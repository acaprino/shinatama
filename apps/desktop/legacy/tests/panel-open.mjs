// The side panels' buttons (Sessions, Overview, Files, Leftovers) sit in the title bar from 1200 up; in a narrower
// window they are items of the toolbar's More menu. A smoke opens a panel the way the person would.
export async function openPanel(page, id, label) {
  const button = page.locator('#' + id);
  if (await button.isVisible()) { await button.click(); return; }
  await page.locator('#toolbar-more').click();
  await page.getByRole('menuitem', { name: label, exact: true }).click();
}
