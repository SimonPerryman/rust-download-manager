/**
 * True while nothing holds focus (a dialog just closed). Focus is only moved
 * for the user then; if they've already clicked or typed elsewhere, it stays.
 */
export function focusIsFree(doc: Pick<Document, "activeElement" | "body"> = document): boolean {
  return doc.activeElement == null || doc.activeElement === doc.body;
}
