export type Theme = { id: string; label: string; hint: string };

/// 情景模式的聊天话题。id 进提示词，hint 只给用户看。
export const THEMES: Theme[] = [
  { id: "gossip", label: "八卦", hint: "聊别人的事，带点夸张和传闻" },
  { id: "smalltalk", label: "闲聊", hint: "天气、周末、最近在忙什么" },
  { id: "dating", label: "约会", hint: "约人出来、试探对方、被放鸽子" },
  { id: "complaint", label: "吐槽", hint: "室友、加班、房东" },
  { id: "shopping", label: "买东西", hint: "砍价、试穿、退换" },
  { id: "food", label: "点餐", hint: "餐厅点单、外卖、忌口" },
  { id: "renting", label: "租房", hint: "看房、押金、合租" },
  { id: "work", label: "上班", hint: "同事、汇报、摸鱼" },
  { id: "study", label: "校园", hint: "考试、作业、社团" },
  { id: "health", label: "看病", hint: "症状、挂号、买药" },
  { id: "travel", label: "旅行", hint: "订票、问路、酒店" },
  { id: "series", label: "追剧", hint: "剧透、追更、吐槽剧情" },
  { id: "gaming", label: "游戏", hint: "组队、开黑、队友" },
  { id: "fitness", label: "健身", hint: "训练计划、饮食、犯懒" },
  { id: "family", label: "家里", hint: "催婚、亲戚、家务" },
  { id: "online", label: "网络用语", hint: "缩写、梗、评论区黑话" },
];
