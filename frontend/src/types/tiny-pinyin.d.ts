declare module 'tiny-pinyin' {
  interface PinyinToken {
    type: number
    source: string
    target: string
  }

  interface PinyinStatic {
    STYLE_NORMAL: number
    STYLE_TONE: number
    STYLE_TONE2: number
    STYLE_TO3NE: number
    STYLE_INITIALS: number
    STYLE_FIRST_LETTER: number

    convertToPinyin(str: string, separator?: string, lowerCase?: boolean): string
    parse(str: string): PinyinToken[]
    isSupported(): boolean
  }

  const pinyin: PinyinStatic
  export default pinyin
}
