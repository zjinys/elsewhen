import type { Item } from './diff';

export const CONTRACT_VERSION = 1;
export const METRICS = {
  stargazers_count: { unit: 'stars', kind: 'counter' },
  forks_count: { unit: 'forks', kind: 'counter' },
  rating_count: { unit: 'ratings', kind: 'counter' },
  ratings_count: { unit: 'ratings', kind: 'counter' },
  rating: { unit: 'rating', kind: 'rating' },
  ratings_average: { unit: 'rating', kind: 'rating' },
  rank: { unit: 'positions', kind: 'rank' },
  price_cents: { unit: 'minor_currency', kind: 'price' },
} as const;

export function productIdentity(source: string, item: Item): { product_id: string; platform: string; platform_key: string } {
  const platform = source.startsWith('github') ? 'github' : source.startsWith('gumroad') ? 'gumroad'
    : source.startsWith('producthunt') ? 'producthunt' : source.startsWith('appstore') ? 'appstore'
    : source.startsWith('cws') ? 'chrome' : source;
  const key = String(item.id);
  return { product_id: JSON.stringify([platform, key]), platform, platform_key: key };
}

export function validateCollection(items: Item[]): void {
  // An empty or partially parsed collection is not evidence that all products disappeared.
  if (!items.length || items.length > 1000) throw new Error('collection_empty_or_over_limit');
  const ids = new Set<string>();
  for (const item of items) {
    if (!item || typeof item.id !== 'string' || !item.id.trim() || item.error || ids.has(item.id)) {
      throw new Error('collection_incomplete_or_duplicate');
    }
    ids.add(item.id);
  }
}

export function titleOf(item: Item): string {
  for (const key of ['name', 'title', 'full_name']) {
    if (typeof item[key] === 'string' && item[key]) return String(item[key]);
  }
  return item.id;
}
