// 发音参考表。静态数据，不参与练习判分——只给用户查。
// 英语按公认发音（RP）整理，韩语按标准语。

export type Phoneme = {
  symbol: string;
  example: string;
  hint: string;
};

export const EN_VOWELS: Phoneme[] = [
  { symbol: "iː", example: "see", hint: "长「衣」，嘴角拉开" },
  { symbol: "ɪ", example: "sit", hint: "短「衣」，放松" },
  { symbol: "e", example: "bed", hint: "「诶」，口型比 æ 小" },
  { symbol: "æ", example: "cat", hint: "大嘴「哎」，下巴低" },
  { symbol: "ɑː", example: "car", hint: "长「啊」，口张开" },
  { symbol: "ɒ", example: "hot", hint: "短「哦」，圆唇" },
  { symbol: "ɔː", example: "door", hint: "长「哦」，圆唇后缩" },
  { symbol: "ʊ", example: "book", hint: "短「乌」，放松" },
  { symbol: "uː", example: "food", hint: "长「乌」，撮唇" },
  { symbol: "ʌ", example: "cup", hint: "短促的「啊」" },
  { symbol: "ɜː", example: "bird", hint: "长「额」，美音带 r" },
  { symbol: "ə", example: "about", hint: "弱读「呃」，最放松" },
];

export const EN_DIPHTHONGS: Phoneme[] = [
  { symbol: "eɪ", example: "day", hint: "诶 → 衣" },
  { symbol: "aɪ", example: "my", hint: "啊 → 衣" },
  { symbol: "ɔɪ", example: "boy", hint: "哦 → 衣" },
  { symbol: "aʊ", example: "now", hint: "啊 → 乌" },
  { symbol: "əʊ", example: "go", hint: "呃 → 乌" },
  { symbol: "ɪə", example: "near", hint: "衣 → 呃" },
  { symbol: "eə", example: "hair", hint: "诶 → 呃" },
  { symbol: "ʊə", example: "tour", hint: "乌 → 呃" },
];

export const EN_CONSONANTS: Phoneme[] = [
  { symbol: "p", example: "pen", hint: "送气 p" },
  { symbol: "b", example: "bed", hint: "不送气 b" },
  { symbol: "t", example: "tea", hint: "送气 t" },
  { symbol: "d", example: "dog", hint: "不送气 d" },
  { symbol: "k", example: "cat", hint: "送气 k" },
  { symbol: "ɡ", example: "go", hint: "不送气 g" },
  { symbol: "tʃ", example: "chair", hint: "「吃」" },
  { symbol: "dʒ", example: "job", hint: "「知」浊音" },
  { symbol: "f", example: "fish", hint: "上齿咬下唇" },
  { symbol: "v", example: "very", hint: "f 的浊音" },
  { symbol: "θ", example: "think", hint: "舌尖在齿间，清" },
  { symbol: "ð", example: "this", hint: "θ 的浊音" },
  { symbol: "s", example: "see", hint: "「丝」" },
  { symbol: "z", example: "zoo", hint: "s 的浊音" },
  { symbol: "ʃ", example: "she", hint: "「诗」" },
  { symbol: "ʒ", example: "measure", hint: "ʃ 的浊音" },
  { symbol: "h", example: "hat", hint: "「喝」" },
  { symbol: "m", example: "man", hint: "双唇鼻音" },
  { symbol: "n", example: "no", hint: "舌尖鼻音" },
  { symbol: "ŋ", example: "sing", hint: "后鼻音「ng」" },
  { symbol: "l", example: "leg", hint: "舌尖抵上齿龈" },
  { symbol: "r", example: "red", hint: "舌尖卷起不碰上颚" },
  { symbol: "j", example: "yes", hint: "半元音「y」" },
  { symbol: "w", example: "we", hint: "半元音「w」" },
];

export type Jamo = {
  letter: string;
  name: string;
  roman: string;
  /** 单独念这个字母时实际发的音：辅音加 ㅡ（그），元音是 ㅇ+元音（아） */
  sound: string;
  example: string;
};

export const KO_CONSONANTS: Jamo[] = [
  { letter: "ㄱ", name: "기역", roman: "g / k", sound: "그", example: "가방" },
  { letter: "ㄲ", name: "쌍기역", roman: "kk", sound: "끄", example: "까치" },
  { letter: "ㄴ", name: "니은", roman: "n", sound: "느", example: "나무" },
  { letter: "ㄷ", name: "디귿", roman: "d / t", sound: "드", example: "다리" },
  { letter: "ㄸ", name: "쌍디귿", roman: "tt", sound: "뜨", example: "딸" },
  { letter: "ㄹ", name: "리을", roman: "r / l", sound: "르", example: "라면" },
  { letter: "ㅁ", name: "미음", roman: "m", sound: "므", example: "모자" },
  { letter: "ㅂ", name: "비읍", roman: "b / p", sound: "브", example: "바다" },
  { letter: "ㅃ", name: "쌍비읍", roman: "pp", sound: "쁘", example: "빵" },
  { letter: "ㅅ", name: "시옷", roman: "s", sound: "스", example: "사과" },
  { letter: "ㅆ", name: "쌍시옷", roman: "ss", sound: "쓰", example: "쌀" },
  { letter: "ㅇ", name: "이응", roman: "– / ng", sound: "응", example: "아이" },
  { letter: "ㅈ", name: "지읒", roman: "j", sound: "즈", example: "지하철" },
  { letter: "ㅉ", name: "쌍지읒", roman: "jj", sound: "쯔", example: "짜다" },
  { letter: "ㅊ", name: "치읓", roman: "ch", sound: "츠", example: "차" },
  { letter: "ㅋ", name: "키읔", roman: "k", sound: "크", example: "커피" },
  { letter: "ㅌ", name: "티읕", roman: "t", sound: "트", example: "토요일" },
  { letter: "ㅍ", name: "피읖", roman: "p", sound: "프", example: "포도" },
  { letter: "ㅎ", name: "히읗", roman: "h", sound: "흐", example: "하늘" },
];

export const KO_VOWELS: Jamo[] = [
  { letter: "ㅏ", name: "아", roman: "a", sound: "아", example: "가" },
  { letter: "ㅐ", name: "애", roman: "ae", sound: "애", example: "개" },
  { letter: "ㅑ", name: "야", roman: "ya", sound: "야", example: "야" },
  { letter: "ㅒ", name: "얘", roman: "yae", sound: "얘", example: "얘" },
  { letter: "ㅓ", name: "어", roman: "eo", sound: "어", example: "서" },
  { letter: "ㅔ", name: "에", roman: "e", sound: "에", example: "세" },
  { letter: "ㅕ", name: "여", roman: "yeo", sound: "여", example: "여" },
  { letter: "ㅖ", name: "예", roman: "ye", sound: "예", example: "예" },
  { letter: "ㅗ", name: "오", roman: "o", sound: "오", example: "고" },
  { letter: "ㅘ", name: "와", roman: "wa", sound: "와", example: "과" },
  { letter: "ㅙ", name: "왜", roman: "wae", sound: "왜", example: "괘" },
  { letter: "ㅚ", name: "외", roman: "oe", sound: "외", example: "회" },
  { letter: "ㅛ", name: "요", roman: "yo", sound: "요", example: "교" },
  { letter: "ㅜ", name: "우", roman: "u", sound: "우", example: "구" },
  { letter: "ㅝ", name: "워", roman: "wo", sound: "워", example: "권" },
  { letter: "ㅞ", name: "웨", roman: "we", sound: "웨", example: "궤" },
  { letter: "ㅟ", name: "위", roman: "wi", sound: "위", example: "귀" },
  { letter: "ㅠ", name: "유", roman: "yu", sound: "유", example: "규" },
  { letter: "ㅡ", name: "으", roman: "eu", sound: "으", example: "그" },
  { letter: "ㅢ", name: "의", roman: "ui", sound: "의", example: "희" },
  { letter: "ㅣ", name: "이", roman: "i", sound: "이", example: "기" },
];

/// 받침 的代表音：收音写法很多，实际只读七个音。
export const KO_FINALS: { letter: string; sound: string; example: string; gloss: string }[] = [
  { letter: "ㄱ ㅋ ㄲ", sound: "[k]", example: "국", gloss: "汤" },
  { letter: "ㄴ ㄵ ㄶ", sound: "[n]", example: "눈", gloss: "眼睛 / 雪" },
  { letter: "ㄷ ㅅ ㅆ ㅈ ㅊ ㅌ ㅎ", sound: "[t]", example: "받", gloss: "收到（词干）" },
  { letter: "ㄹ ㄼ ㄽ ㄾ ㅀ", sound: "[l]", example: "말", gloss: "话 / 马" },
  { letter: "ㅁ ㄻ", sound: "[m]", example: "밤", gloss: "夜晚 / 栗子" },
  { letter: "ㅂ ㅍ ㄿ", sound: "[p]", example: "밥", gloss: "饭" },
  { letter: "ㅇ", sound: "[ŋ]", example: "강", gloss: "江" },
];