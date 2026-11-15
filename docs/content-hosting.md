# 自建内容仓库

词表和内容不必依赖别人的仓库。建议单独建一个公开仓库（例如 Gitee）专放 `books/`：
词表 JSON、句库 TSV、阅读文章都在里面，和程序仓库分开，随时能加内容而不用发版。

```
books/
├── index.json
├── words/en-cet4.json       # 词表：qwerty-learner 格式或 {"name","trans"} 数组
├── sentences/en-daily.tsv   # 句库：surface<TAB>释义
└── articles/en-tech-01.md   # 文章：见下方「文章格式」
```

仓库根推送后，raw 地址形如 `https://gitee.com/<用户>/<仓库>/raw/master/books/index.json`
（Gitee 会 302 到签名地址，程序跟随跳转即可）。在词库页「索引设置」填这个地址，
或直接写多行地址，前一个失败自动换下一个。

每条书目可以带 `mirrors`（备用地址数组），主地址挂了照样能装：

```json
{
  "id": "en-daily",
  "lang": "en",
  "kind": "sentences",
  "name": "英语日常句",
  "url": "https://gitee.com/you/ealearn-content/raw/master/books/sentences/en-daily.tsv",
  "mirrors": ["https://raw.githubusercontent.com/you/ealearn-content/main/books/sentences/en-daily.tsv"]
}
```

把现有索引里的资源抓成本地副本、同时把 `url` 改写成自建仓地址（原地址进 `mirrors`）：

```sh
scripts/mirror-books.sh https://gitee.com/you/ealearn-content/raw/master --out /tmp/ealearn-books
# 然后把 /tmp/ealearn-books/books 整个推上去
```

阅读材料和句库可以直接让模型批量生成：文章按 `# ? - =` 约定写，句子写成
`surface<TAB>释义`，装进仓库、在索引里登记即可。句子至少要 3 个词才会进拼句，
导入不校验语法，但分词对不上的文章会被拒。

## 索引字段

`index.json` 是 `{"version": 6, "books": [...]}`，每条书目：

| 字段 | 说明 |
|---|---|
| `id` | 唯一标识 |
| `lang` | `en` / `ko`，对应语言包 |
| `kind` | `words`（默认）/ `sentences` / `articles` |
| `name` | 词库页显示的名字，也是「已下载」计数的键 |
| `url` | 内容地址，http(s)、绝对路径、`books/` 下的相对路径都行 |
| `mirrors` | 备用地址数组，前面的失败依次重试 |
| `count` / `license` / `note` | 展示用 |

词表支持 TSV/CSV、JSON 数组和 JSONL（`word`/`name`/`korean_term` +
`trans`/`translation`/`meaning`/`english_term`）。改完索引记得 bump `version`。

## 文章格式

```
# 标题
正文第一段
第二段

? 题干
- 选项 A
- 选项 B
= 2
```

`#` 标题、`?` 题干、`-` 选项、`=` 正确选项序号（从 1 开始），其余行都是正文，空行分段。
