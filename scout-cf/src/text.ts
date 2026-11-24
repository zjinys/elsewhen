import { parseDocument } from 'htmlparser2';

const blocks = new Set(['p', 'div', 'li', 'br', 'section', 'article', 'h1', 'h2', 'h3', 'tr']);
const hidden = new Set(['script', 'style', 'template', 'noscript']);
const navigation = new Set(['discussion', 'link', 'comments']);
type Node = ReturnType<typeof parseDocument>['children'][number];

/** RSS may escape an already escaped HTML fragment. Decode bounded layers. */
export function plainText(value: string, feedNavigation = false): string {
  let text = value;
  for (let layer = 0; layer < 4; layer++) {
    const gather = (node: Node): string => {
      if (node.type === 'text') return node.data;
      if (!('children' in node)) return '';
      const tag = 'name' in node ? node.name : '';
      if (hidden.has(tag)) return '';
      const content = node.children.map(gather).join('');
      if (feedNavigation && tag === 'a' && navigation.has(content.trim().toLowerCase())) return '';
      return blocks.has(tag) ? ` ${content} ` : content;
    };
    const cleaned = parseDocument(text).children.map(gather).join('').replace(/\s+/g, ' ').trim();
    if (cleaned === text) break;
    text = cleaned;
  }
  if (feedNavigation) {
    text = text.replace(/(?:\s*\|\s*(?:Discussion|Link|Comments))+\s*$/gi, '')
      .replace(/(?:\s*\|\s*)+$/, '').trim();
  }
  return text;
}
