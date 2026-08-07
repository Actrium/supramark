import type { SelectionRange, SelectionUnit } from '../model';
import { serializeSelectionUnits, type SelectionSerializeFormat } from '../serialize';
import type { SelectionToolbarItem } from './toolbar';

/**
 * A host-facing copy request emitted when a toolbar action fires.
 *
 * The shape is unchanged from when these requests originated in a *native*
 * menu action (`blockSink.ts`, removed with the native command bridge): the
 * package still never touches a clipboard library, it serializes the covered
 * range in the requested format and hands the result to the host. What changed
 * is only where the action comes from — a React button we own rather than a
 * `UIAction` appended to a system menu.
 */
export interface SelectionCopyRequest {
  /** The toolbar item's id. */
  id: string;
  format: SelectionSerializeFormat;
  /** Serialized in `format`; `undefined` when the format yields nothing. */
  payload: string | Uint8Array | undefined;
  /** `plainText` convenience — always a string, `''` when nothing is covered. */
  text: string;
  /** The document range that was copied. */
  range: SelectionRange;
}

/**
 * Build the request for one toolbar action against an already-resolved
 * selection. Pure: the caller owns both the store snapshot it reads from and
 * the delivery of the result.
 *
 * An item with no `format` still produces a request (`plainText`), so a host
 * action such as "Ask AI" or "Quote" gets the selected text without having to
 * re-serialize it.
 */
export function buildCopyRequest(
  item: SelectionToolbarItem,
  units: readonly SelectionUnit[],
  range: SelectionRange
): SelectionCopyRequest {
  const format = item.format ?? 'plainText';
  const payload = serializeSelectionUnits(units, format);
  const text = serializeSelectionUnits(units, 'plainText');
  return {
    id: item.id,
    format,
    payload,
    text: typeof text === 'string' ? text : '',
    range,
  };
}
