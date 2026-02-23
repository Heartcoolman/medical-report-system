import Pinyin from 'tiny-pinyin'

/**
 * Convert a string to its full pinyin representation (lowercase, no separator).
 * Non-Chinese characters are kept as-is.
 */
export function toPinyinFull(text: string): string {
  return Pinyin.convertToPinyin(text, '', true)
}

/**
 * Convert a string to its pinyin initials (first letter of each Chinese character's pinyin).
 * Non-Chinese characters are kept as-is.
 */
export function toPinyinInitials(text: string): string {
  const tokens = Pinyin.parse(text)
  let result = ''
  for (const token of tokens) {
    if (token.type === 2) {
      // Chinese character — take first letter of pinyin
      result += token.target.charAt(0).toLowerCase()
    } else {
      // Non-Chinese — keep as-is, lowercased
      result += token.source.toLowerCase()
    }
  }
  return result
}

/**
 * Check if `text` matches `query` using three-layer matching:
 * 1. Original text contains query (case-insensitive)
 * 2. Full pinyin contains query
 * 3. Pinyin initials contain query
 */
export function pinyinMatch(text: string, query: string): boolean {
  if (!query) return true
  const q = query.toLowerCase()

  // Layer 1: original text contains
  if (text.toLowerCase().includes(q)) return true

  // Layer 2: full pinyin contains
  if (toPinyinFull(text).includes(q)) return true

  // Layer 3: pinyin initials contains
  if (toPinyinInitials(text).includes(q)) return true

  return false
}
