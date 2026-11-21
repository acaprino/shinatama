// Imago's field part is a <label> around its text and its control. A control inside its label is
// named by the label's whole text, its own value included: the Theme select read "Theme Windows",
// and Settings' fields could no longer be found by their names. The visible text names the control
// alone, as the `for` labels did before the Imago migration.
let next = 0;

export function labelledBy<T extends HTMLElement>(text: HTMLElement, control: T): T {
  if (!text.id) text.id = `field-label-${++next}`;
  control.setAttribute('aria-labelledby', text.id);
  return control;
}
