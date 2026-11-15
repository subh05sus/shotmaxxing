/**
 * The Home window's one search, in its top bar. Typing filters the Library while it shows, and the list under the
 * field offers the first matching recordings and actions. ⌘K jumps into it; other windows keep the ⌘K palette.
 */

/** A recording or screenshot the Library shows for the search, first ones first. */
export interface SearchMatch {
  id: string;
  title: string;
  detail?: string;
  kind: 'recording' | 'screenshot';
  open: () => void;
}

export const homeSearch = $state({ query: '', focus: 0, matches: [] as SearchMatch[] });

/** Puts the cursor in the search, selecting what's there (⌘K in the Home window). */
export function focusHomeSearch(): void {
  homeSearch.focus++;
}
