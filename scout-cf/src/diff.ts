/**
 * Deterministic diff between consecutive snapshots: new / changed / gone.
 * Content hash over the whole item (sorted-key JSON) — same semantics as
 * the Python service.
 */

/** 单个字段的变化：`from` → `to`；两者都是有限数时附 `diff`（to - from）。 */
export interface FieldDelta {
  from: unknown;
  to: unknown;
  diff?: number;
}

export interface Finding {
  kind: "new" | "changed" | "gone";
  item_key: string;
  payload: unknown;
  /**
   * 仅 `changed` 有：字段级变化。
   * 「涨了多少」比「现在是多少」是更高阶的信号——光看 payload 里的
   * stars=13356 无法判断它是刚涨的还是本来就在那。
   */
  delta?: Record<string, FieldDelta>;
}

export type Item = Record<string, unknown> & { id: string };

function stableStringify(obj: unknown): string {
  if (obj === null || typeof obj !== "object") return JSON.stringify(obj) ?? "null";
  if (Array.isArray(obj)) return "[" + obj.map(stableStringify).join(",") + "]";
  const keys = Object.keys(obj as Record<string, unknown>).sort();
  return (
    "{" +
    keys
      .map((k) => JSON.stringify(k) + ":" + stableStringify((obj as Record<string, unknown>)[k]))
      .join(",") +
    "}"
  );
}

export function itemHash(item: Item): string {
  const s = stableStringify(item);
  // SHA-256 via WebCrypto would be async; for diff purposes a fast stable
  // non-crypto hash is enough (FNV-1a 64-bit over the canonical string).
  let h1 = 0x811c9dc5n;
  let h2 = 0x01000193n;
  for (let i = 0; i < s.length; i++) {
    const c = BigInt(s.charCodeAt(i));
    h1 = ((h1 ^ c) * 0x01000193n) & 0xffffffffffffffffn;
    h2 = ((h2 + c) * 0x00000100_000001b3n) & 0xffffffffffffffffn;
  }
  return h1.toString(16) + h2.toString(16);
}

/** 字段级 diff。只比 id 之外的字段；数量上限防止 payload 膨胀。 */
function computeDelta(prev: Item, curr: Item): Record<string, FieldDelta> | undefined {
  const out: Record<string, FieldDelta> = {};
  let n = 0;
  const keys = new Set([...Object.keys(prev), ...Object.keys(curr)]);
  for (const k of keys) {
    if (k === "id") continue;
    const a = prev[k];
    const b = curr[k];
    if (stableStringify(a) === stableStringify(b)) continue;
    const d: FieldDelta = { from: a ?? null, to: b ?? null };
    if (typeof a === "number" && typeof b === "number" && isFinite(a) && isFinite(b)) {
      d.diff = b - a;
    }
    out[k] = d;
    if (++n >= 12) break;
  }
  return n > 0 ? out : undefined;
}

export function diffItems(current: Item[], previous: Item[] | null): Finding[] {
  const prevMap = new Map<string, string>();
  if (previous) {
    for (const it of previous) prevMap.set(String(it.id), itemHash(it));
  }

  const prevItems = new Map<string, Item>();
  if (previous) {
    for (const it of previous) prevItems.set(String(it.id), it);
  }

  const findings: Finding[] = [];
  for (const item of current) {
    if (item.id === undefined || item.id === null) {
      throw new Error("normalized item missing stable 'id' field");
    }
    const key = String(item.id);
    const h = itemHash(item);
    if (!prevMap.has(key)) {
      findings.push({ kind: "new", item_key: key, payload: item });
    } else if (prevMap.get(key) !== h) {
      const before = prevItems.get(key);
      const delta = before ? computeDelta(before, item) : undefined;
      findings.push(
        delta
          ? { kind: "changed", item_key: key, payload: item, delta }
          : { kind: "changed", item_key: key, payload: item }
      );
    }
  }
  if (previous) {
    // gone 的 payload 带上最后一次见到的完整 item——消费端 UI 才能
    // 显示「什么消失了」，而不是一串裸 id。
    for (const key of prevMap.keys()) {
      if (!current.some((i) => String(i.id) === key)) {
        const last = prevItems.get(key);
        findings.push({
          kind: "gone",
          item_key: key,
          payload: last ?? { id: key },
        });
      }
    }
  }
  return findings;
}
