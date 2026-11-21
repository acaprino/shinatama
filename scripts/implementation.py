"""Validate and render the resumable implementation plan. Standard library only."""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "docs/implementation"
STATUSES = {"pending", "in_progress", "verified", "committed", "blocked", "deferred"}
DONE = {"verified", "committed"}


def load():
    plan = json.loads((DIRECTORY / "plan.json").read_text(encoding="utf-8"))
    progress = json.loads((DIRECTORY / "progress.json").read_text(encoding="utf-8"))
    return plan, progress


def requirements():
    found = {}
    for path in sorted((ROOT / "docs/frd").glob("FRD-*.md")):
        match = re.match(r"(FRD-\d{4})-", path.name)
        if match:
            for row in re.finditer(r"^\| (R\d+) \|", path.read_text(encoding="utf-8"), re.M):
                found[f"{match[1]}/{row[1]}"] = path
    return found


def _reason(value):
    return isinstance(value, str) and bool(value.strip())


def validate(plan, progress):
    steps = plan["steps"]
    by_id = {step["id"]: step for step in steps}
    errors = []
    if plan.get('schema_version') != 1 or progress.get('schema_version') != 1:
        errors.append('Unsupported plan/checkpoint schema version; preserve files and migrate explicitly')
    if len(by_id) != len(steps):
        errors.append("Duplicate step IDs")
    known = requirements()
    covered = set()
    unit_ids = set()
    for step in steps:
        if step.get('scope') not in {'v1', 'post-v1', 'evaluation', 'deferred'}:
            errors.append(f"{step['id']}: invalid scope")
        if step.get('scope') == 'deferred' and not step.get('deferred_reason'):
            errors.append(f"{step['id']}: deferred scope needs the user's reason")
        for field in ("title", "deliverable", "acceptance", "checks", "files", "gates", "hours", "scope", "depends_on", "requirements", "commit", "resume"):
            if field not in step or (not step[field] and field not in {"depends_on", "requirements", "gates"}):
                errors.append(f"{step['id']}: missing {field}")
        for requirement in step["requirements"]:
            if requirement not in known:
                errors.append(f"{step['id']}: unknown requirement {requirement}")
            covered.add(requirement)
        for dependency in step["depends_on"]:
            if dependency not in by_id:
                errors.append(f"{step['id']}: unknown dependency {dependency}")
            elif step.get('scope') == 'v1' and by_id[dependency].get('scope') == 'deferred':
                errors.append(f"{step['id']}: active scope cannot depend on deferred step {dependency}")
        if len(step["hours"]) != 2 or not 0 <= step["hours"][0] <= step["hours"][1]:
            errors.append(f"{step['id']}: invalid hour range")
        for unit in step.get('commit_units', []):
            if unit['id'] in unit_ids:
                errors.append(f"Duplicate commit-unit ID: {unit['id']}")
            unit_ids.add(unit['id'])
            for field in ('title', 'deliverable', 'acceptance', 'checks', 'files', 'hours', 'commit'):
                if not unit.get(field):
                    errors.append(f"{unit['id']}: missing {field}")
            if len(unit['hours']) != 2 or not 0 < unit['hours'][0] <= unit['hours'][1] <= 12:
                errors.append(f"{unit['id']}: commit units must be bounded to 12 hours")
        if step['hours'][1] > 16 and not step.get('commit_units'):
            errors.append(f"{step['id']}: large increment needs explicit commit units")
    for missing in sorted(known.keys() - covered):
        errors.append(f"Unplanned requirement: {missing}")
    visiting, visited = set(), set()

    def visit(step_id):
        if step_id in visiting:
            errors.append(f"Dependency cycle at {step_id}")
            return
        if step_id in visited or step_id not in by_id:
            return
        visiting.add(step_id)
        for dependency in by_id[step_id]["depends_on"]:
            visit(dependency)
        visiting.remove(step_id)
        visited.add(step_id)
    for step_id in by_id:
        visit(step_id)
    active = []
    for step_id, state in progress["steps"].items():
        if step_id not in by_id or state["status"] not in STATUSES:
            errors.append(f"Invalid checkpoint: {step_id}")
        if state["status"] == "in_progress":
            active.append(step_id)
            if by_id.get(step_id, {}).get('scope') == 'deferred':
                errors.append(f"{step_id}: deferred work needs a new user instruction before resuming")
        if state["status"] == "in_progress" and by_id.get(step_id, {}).get("scope") != "v1" and step_id not in progress.get("allow_non_v1", []):
            errors.append(f"{step_id}: non-V1 work needs explicit opt-in in allow_non_v1")
        if state["status"] in DONE:
            if not state.get("evidence") or not state.get("checks"):
                errors.append(f"{step_id}: completion needs evidence and executed checks")
            for path in state.get("evidence", []):
                if not (ROOT / path).is_file():
                    errors.append(f"{step_id}: evidence file missing: {path}")
        # Work recorded before a dependency was verified is kept as it happened: each such
        # dependency is named with the reason, never silently treated as verified.
        exceptions = state.get("accepted_before_dependencies") or {}
        for dependency, reason in exceptions.items():
            if dependency not in by_id.get(step_id, {}).get("depends_on", []):
                errors.append(f"{step_id}: exception for {dependency}, which is not a dependency")
            if not _reason(reason):
                errors.append(f"{step_id}: exception for {dependency} needs its reason")
        if state["status"] in DONE | {"in_progress"}:
            for dependency in by_id.get(step_id, {}).get("depends_on", []):
                if progress["steps"].get(dependency, {}).get("status") not in DONE and not _reason(exceptions.get(dependency)):
                    errors.append(f"{step_id}: dependency {dependency} is not verified")
        if state["status"] == "committed" and not re.fullmatch(r"[0-9a-f]{40}", state.get("commit_sha") or ""):
            errors.append(f"{step_id}: committed needs the real full SHA")
        elif state["status"] == "committed":
            result = subprocess.run(["git", "cat-file", "-t", state["commit_sha"]], cwd=ROOT, capture_output=True, text=True)
            if result.returncode != 0 or result.stdout.strip() != "commit":
                errors.append(f"{step_id}: commit SHA is not an existing Git commit")
        units = by_id.get(step_id, {}).get('commit_units', [])
        unit_states = state.get('units', {})
        for unknown in unit_states.keys() - {unit['id'] for unit in units}:
            errors.append(f"{step_id}: unknown unit checkpoint {unknown}")
        previous_done = True
        active_units = 0
        for unit in units:
            unit_state = unit_states.get(unit['id'], {'status': 'pending'})
            status = unit_state.get('status')
            if status not in STATUSES:
                errors.append(f"{unit['id']}: invalid unit status")
            if status == 'in_progress':
                active_units += 1
                if state['status'] != 'in_progress' or not previous_done:
                    errors.append(f"{unit['id']}: active unit requires active parent and completed previous units")
            if status in DONE:
                in_order = previous_done or _reason(unit_state.get('accepted_before_previous_units'))
                if not in_order or not unit_state.get('checks') or not unit_state.get('evidence'):
                    errors.append(f"{unit['id']}: completion needs prior units, executed checks and evidence")
                for path in unit_state.get('evidence', []):
                    if not (ROOT / path).is_file():
                        errors.append(f"{unit['id']}: missing unit evidence {path}")
                if status == 'committed':
                    sha = unit_state.get('commit_sha') or ''
                    result = subprocess.run(['git', 'cat-file', '-t', sha], cwd=ROOT, capture_output=True, text=True) if re.fullmatch(r'[0-9a-f]{40}', sha) else None
                    if not result or result.returncode or result.stdout.strip() != 'commit':
                        errors.append(f"{unit['id']}: committed unit needs an existing Git commit")
            previous_done = previous_done and status in DONE
        if active_units > 1:
            errors.append(f"{step_id}: only one active commit unit allowed")
        if units and state['status'] in DONE and not previous_done:
            errors.append(f"{step_id}: parent completion requires every commit unit verified")
    if len(active) > 1:
        errors.append(f"Only one active step allowed: {active}")
    return errors, len(known), len(covered & known.keys())


def next_step(plan, progress):
    states = progress["steps"]
    def ready(step):
        return step['scope'] != 'deferred' and (step["scope"] == "v1" or step["id"] in progress.get("allow_non_v1", [])) and all(states.get(d, {}).get("status") in DONE for d in step["depends_on"])
    active = next((s for s in plan["steps"] if states.get(s["id"], {}).get("status") == "in_progress" and ready(s)), None)
    if active:
        return active
    return next((s for s in plan["steps"] if s["scope"] == "v1"
        and states.get(s["id"], {}).get("status", "pending") == "pending"
        and all(states.get(d, {}).get("status") in DONE for d in s["depends_on"])), None)


def render(plan, progress):
    lines = ["# Piano completo di implementazione", "", "Generato da `plan.json` e `progress.json` con `python scripts/implementation.py render`.",
        "Le specifiche FRD restano l'autorità sui requisiti; il registro di verifica resta l'autorità sulle prove.", "",
        "## Ripresa immediata", "", "Eseguire `python scripts/implementation.py status`, poi leggere la scheda del passo indicato.",
        "Aggiornare `progress.json` alla fine di ogni sessione; `verified` significa incremento verificato sulle piattaforme registrate, non tutti i gate FRD superati.", "",
        "## Regole comuni per ogni incremento", "",
        "1. Verificare prerequisiti e modifiche locali; delimitare i file del passo. Nessuno `git add .` in un checkout condiviso.",
        "2. Implementare una capacità utilizzabile; mantenere avviabile il programma e mostrare come indisponibili le capacità non provate.",
        "3. Eseguire solo le verifiche essenziali al cambiamento secondo la policy sotto; registrare versione, OS e risultato. I gate senza prova restano aperti.",
        "4. Aggiornare brevemente evidenza e checkpoint; selezionare solo i file del passo e creare un commit funzionale dopo le verifiche essenziali.",
        "5. Riprendere dal primo passo pronto. Per annullare un passo già committato usare un commit di revert; le migrazioni richiedono backup e compatibilità, mai rollback distruttivo del DB.", "",
        "I comandi nelle schede future sono verifiche pianificate: i relativi test e comandi devono essere creati nel passo prima di poter dichiarare il risultato.",
        "Le stime sono ore di lavoro tecnico, includono implementazione/verifica/revisione e non costituiscono date di consegna; si aggiornano con le prove delle integrazioni.", "",
        "## Sequenza e dipendenze", "", "| Passo | Risultato | Prerequisiti | Stato | Ore |", "| --- | --- | --- | --- | --- |"]
    policy = plan.get('execution_policy')
    if policy:
        position = lines.index('## Sequenza e dipendenze')
        policy_lines = ['', '## Verifiche essenziali per incremento', '', policy['authorization'], '']
        policy_lines += [f'{index}. {check}' for index, check in enumerate(policy['essential_checks'], 1)]
        policy_lines += ['', policy['extended_checks'], '', policy['stability_checks'], '', policy['existing_check_catalog'], '', policy['checkpoint'], '']
        lines[position:position] = policy_lines
    delivery = plan.get('delivery_request')
    if delivery:
        position = lines.index('## Sequenza e dipendenze')
        delivery_lines = ['', '## Consegna richiesta: mattina del 3 novembre', '', delivery['user_requested_scope'], '', delivery['execution_priority'], '']
        delivery_lines += [f'{index}. {criterion}' for index, criterion in enumerate(delivery['ready_criteria'], 1)]
        delivery_lines += ['', delivery['scope_rule'], '', delivery['assessment'], '', 'Avanzamento: contare separatamente gli incrementi V1 verificati e le unita del blocco attivo; il numero di passi non e una percentuale del tempo o del prodotto.', '']
        lines[position:position] = delivery_lines
    for s in plan["steps"]:
        state = progress["steps"].get(s["id"], {}).get("status", "pending")
        lines.append(f"| [{s['id']}](#{s['id'].lower()}) | {s['title']} | {', '.join(s['depends_on']) or '—'} | {state} | {s['hours'][0]}–{s['hours'][1]} |")
    if plan.get('scope_overrides'):
        scope_lines = ['', '## Rinvii richiesti dall’utente', '']
        for override in plan['scope_overrides']:
            scope_lines += [override['description'], '']
        # Put the current scope before the long dependency table.
        position = lines.index('## Sequenza e dipendenze')
        lines[position:position] = scope_lines
    for s in plan["steps"]:
        lines += ["", f"<a id=\"{s['id'].lower()}\"></a>", f"## {s['id']} · {s['title']}", "", s["deliverable"], "",
            f"**Ambito:** {s['scope']}. **Dipendenze:** {', '.join(s['depends_on']) or 'nessuna'}. **Stima:** {s['hours'][0]}–{s['hours'][1]} ore.", "",
            "**File/componenti:** " + "; ".join(f"`{f}`" for f in s["files"]) + ".", "",
            "**Accettazione osservabile:**", ""]
        if s.get('deferred_reason'):
            lines += [f"**Rinvio:** {s['deferred_reason']}", '']
        if s.get('deferred_clauses'):
            lines += ['**Parti rinviate nello scope corrente:** ' + ' '.join(s['deferred_clauses']), '']
        lines += [f"- {a}" for a in s["acceptance"]]
        lines += ["", "**Catalogo verifiche (applicare la policy essenziale):** " + "; ".join(f"`{c}`" for c in s["checks"]) + ".", "",
            "**Gate collegati, da registrare separatamente:** " + (", ".join(s["gates"]) or "nessun gate intero chiuso da questo passo") + ".", "",
            "**Requisiti coperti dal piano:** " + (", ".join(s["requirements"]) or "infrastruttura di esecuzione") + ".", "",
            f"**Commit proposto:** `{s['commit']}`.", "", f"**Prima azione alla ripresa:** {s['resume']}"]
        if s.get('commit_units'):
            lines += ['', '**Unita di commit sequenziali (ognuna lascia l’app avviabile):**']
            for unit in s['commit_units']:
                lines += ['', f"### {unit['id']} · {unit['title']}", '', unit['deliverable'], '',
                    f"Stima: {unit['hours'][0]}–{unit['hours'][1]} ore. File: " + ', '.join(f'`{f}`' for f in unit['files']) + '.', '',
                    'Accettazione: ' + ' '.join(unit['acceptance']), '',
                    'Verifiche pianificate: ' + '; '.join(f'`{c}`' for c in unit['checks']) + '.', '',
                    f"Commit: `{unit['commit']}`. Registrare stato e prove sotto `progress.json` → `steps` → `{s['id']}` → `units` → `{unit['id']}`."]
    lines += ["", "## Copertura e confini", "",
        "Ogni requisito Markdown FRD-0001–FRD-0017 è assegnato ad almeno un passo, compresi invarianti, esclusioni e prove. Assegnazione non equivale a implementazione.",
        "Gli incrementi `evaluation` possono concludersi con una decisione o una capacità ancora indisponibile; non dichiarano consegnata la funzionalità. Gli incrementi `post-v1` sono esclusi dal prossimo passo automatico.",
        "Hop V1 solo seed/resume; nessuna scrittura di sessioni native in V1, nessun Git Diff dedicato, Grok o fallback browser automatico. Widget ultimo nella propria sequenza V1; browser/roundtable/TokenEconomizer non bloccano S1 e S2.",
        "Tauri è la shell V1 definitiva (ADR-0005 v2, ADR-0047 v2); TL1-proof la prova su Windows, unico sistema supportato ufficialmente. Le revisioni dipendenti di ADR-0002 (Mermaid), ADR-0040 e ADR-0048 restano da approvare.", ""]
    if plan.get('scope_overrides'):
        lines += ['I rinvii espliciti dell’utente prevalgono sullo scope storico: le parti rinviate restano tracciate, escluse dalla selezione automatica e dall’accettazione del ciclo attivo.', '']
    (DIRECTORY / "PLAN.md").write_text("\n".join(lines), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["validate", "status", "next", "render"])
    args = parser.parse_args()
    plan, progress = load()
    errors, total, covered = validate(plan, progress)
    if errors:
        for error in errors:
            print(error)
        raise SystemExit(1)
    if args.command == "render":
        render(plan, progress)
        print("Rendered docs/implementation/PLAN.md")
    elif args.command in {"status", "next"}:
        step = next_step(plan, progress)
        completed = sum(state["status"] in DONE for state in progress["steps"].values())
        v1 = [item for item in plan['steps'] if item['scope'] == 'v1']
        v1_done = sum(progress['steps'].get(item['id'], {}).get('status') in DONE for item in v1)
        print(f"Verified V1 increments: {v1_done}/{len(v1)}; all-scope checkpoints: {completed}/{len(plan['steps'])}; requirements mapped: {covered}/{total} (not product completion)")
        if step:
            print(f"Next: {step['id']} -- {step['title']}")
            print(step["resume"])
            unit = next((u for u in step.get('commit_units', []) if progress['steps'].get(step['id'], {}).get('units', {}).get(u['id'], {}).get('status') not in DONE), None)
            if unit:
                print(f"Commit unit: {unit['id']} -- {unit['title']}")
        else:
            print("No V1 step currently ready; inspect dependencies/blockers in progress.json")
    else:
        print(f"PASS: {len(plan['steps'])} steps; {covered}/{total} requirements mapped; dependencies acyclic; checkpoint valid")


if __name__ == "__main__":
    main()
