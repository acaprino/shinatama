//! F04: the instance overview (FRD-0010 R01-R03, R09, R12, R14). Read-mostly: it reports what
//! Shinatama observed, with the source of each state, and offers only Focus. No native agent
//! hook feeds it yet, so every running state is labelled unsure; only "exited" is certain.
//! `waiting_permission` is never inferred from screen text.
use crate::{Host, attention};
use serde::Serialize;
use shinatama_projects::git::{self, GitCache, GitFacts};
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{State, WebviewWindow};

fn cache() -> &'static GitCache {
    static CACHE: OnceLock<GitCache> = OnceLock::new();
    CACHE.get_or_init(GitCache::default)
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Starting,
    AttentionRequested,
    ProducingOutput,
    Quiet,
    Exited,
    Error,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StateSource {
    /// The process ended: certain.
    Process,
    /// BEL / OSC 9 / OSC 777 seen in the stream: the terminal asked for attention, nothing more.
    TerminalSignal,
    /// Bytes arrived recently.
    TerminalOutput,
    /// Nothing observed yet.
    None,
}

pub fn derive(observation: Option<attention::TerminalObservation>) -> (InstanceState, StateSource, bool) {
    let Some(o) = observation else { return (InstanceState::Starting, StateSource::None, false) };
    if o.ended {
        return (if o.exit_code.is_some_and(|c| c != 0) { InstanceState::Error } else { InstanceState::Exited }, StateSource::Process, true);
    }
    let signal_after_output = match (o.since_signal_ms, o.since_output_ms) {
        (Some(signal), Some(output)) => signal <= output.saturating_add(1_000),
        (Some(_), None) => true,
        _ => false,
    };
    if signal_after_output && o.since_signal_ms.is_some_and(|ms| ms < 30 * 60 * 1000) {
        return (InstanceState::AttentionRequested, StateSource::TerminalSignal, false);
    }
    match o.since_output_ms {
        Some(ms) if ms < 3_000 => (InstanceState::ProducingOutput, StateSource::TerminalOutput, false),
        Some(_) => (InstanceState::Quiet, StateSource::TerminalOutput, false),
        None => (InstanceState::Starting, StateSource::None, false),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    id: String,
    agent: String,
    cwd: String,
    state: InstanceState,
    state_source: StateSource,
    certain: bool,
    git: GitFacts,
    usage: &'static str,
}

#[tauri::command]
pub async fn instances_overview(window: WebviewWindow, host: State<'_, Host>, feed: State<'_, attention::Controller>) -> Result<Vec<Row>, String> {
    crate::local(&window)?;
    let sessions = host.list();
    let observations = feed.observations();
    let rows: Vec<(shinatama_terminal::SessionInfo, (InstanceState, StateSource, bool))> =
        sessions.into_iter().map(|s| { let d = derive(observations.get(&s.id).copied()); (s, d) }).collect();
    tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = rows.iter().map(|(s, _)| PathBuf::from(&s.cwd)).collect();
        let facts = cache().scan(&paths, git::DEFAULT_TIMEOUT);
        Ok(rows
            .into_iter()
            .zip(facts)
            .map(|((s, (state, state_source, certain)), git)| Row {
                id: s.id,
                agent: s.shell,
                cwd: s.cwd,
                state,
                state_source,
                certain,
                git,
                usage: "Tokens and cost: open the session detail (from the agent's own records).",
            })
            .collect())
    })
    .await
    .map_err(|_| "The overview worker stopped.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention::TerminalObservation;

    fn o(ended: bool, exit: Option<u32>, signal: Option<u64>, output: Option<u64>) -> Option<TerminalObservation> {
        Some(TerminalObservation { ended, exit_code: exit, since_signal_ms: signal, since_output_ms: output })
    }

    #[test]
    fn only_process_exit_is_certain_and_no_permission_state_is_inferred() {
        assert_eq!(derive(None), (InstanceState::Starting, StateSource::None, false));
        assert_eq!(derive(o(true, Some(0), None, Some(10))), (InstanceState::Exited, StateSource::Process, true));
        assert_eq!(derive(o(true, Some(1), None, Some(10))), (InstanceState::Error, StateSource::Process, true));
        assert_eq!(derive(o(false, None, Some(100), Some(5_000))), (InstanceState::AttentionRequested, StateSource::TerminalSignal, false));
        assert_eq!(derive(o(false, None, Some(60_000), Some(500))), (InstanceState::ProducingOutput, StateSource::TerminalOutput, false), "output after the signal wins");
        assert_eq!(derive(o(false, None, None, Some(10_000))), (InstanceState::Quiet, StateSource::TerminalOutput, false));
        for observation in [o(false, None, Some(1), Some(1)), o(false, None, None, Some(1))] {
            assert!(!derive(observation).2, "a running state is never certain");
        }
    }
}
