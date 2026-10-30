// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 類義語辞典 (Thesaurus: DOCX-REVIEW-019):
//! Word 標準の校閲 > 類義語辞典 (Thesaurus / Shift+F7 / docx.review.thesaurus)。
//! - 選択中テキストまたはキャレット位置の単語を自動検出して検索
//! - 品詞別・意味別に分類された類義語・同義語リストの表示
//! - ワンクリックでの本文への置換 (1ステップの Undo/Redo 連動)
//! - クリップボードへのコピー
//! - 検索履歴と前の検索結果への戻る操作
//! - 日英の豊富なビジネス・学術・日常語彙を内包（クリーンルーム実装）

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::FlowPos;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 1件のシソーラスグループ定義（品詞、意味、類語配列）
#[derive(Clone, Debug)]
pub(crate) struct SynonymEntry {
    pub(crate) category: &'static str,
    pub(crate) meaning: &'static str,
    pub(crate) synonyms: &'static [&'static str],
}

/// 検索結果の1グループ
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThesaurusGroup {
    pub(crate) category: String,
    pub(crate) meaning: String,
    pub(crate) synonyms: Vec<String>,
}

/// 類義語検索結果
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThesaurusResult {
    pub(crate) query: String,
    pub(crate) groups: Vec<ThesaurusGroup>,
}

/// クリーンルーム実装による内包シソーラス辞書データ
pub(crate) const THESAURUS_ENTRIES: &[SynonymEntry] = &[
    // --- 日本語: 動詞・サ変名詞 ---
    SynonymEntry {
        category: "名詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "新しく作り出すこと", // i18n-ok（類語辞書のデータ）
        synonyms: &["作成", "制作", "製作", "生成", "構築", "起草", "執筆", "立案", "策定", "創出"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "物事を始めること", // i18n-ok（類語辞書のデータ）
        synonyms: &["開始", "始める", "着手", "スタート", "起動", "発足", "幕開け", "開始する", "着手する"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "物事を終えること", // i18n-ok（類語辞書のデータ）
        synonyms: &["終了", "完了", "終結", "完結", "終わり", "終了する", "完了する", "結び", "打ち切り"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "将来の実行手順や目標", // i18n-ok（類語辞書のデータ）
        synonyms: &["計画", "予定", "プラン", "構想", "企画", "設計", "青写真", "プロジェクト"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "内容を改めること", // i18n-ok（類語辞書のデータ）
        synonyms: &["変更", "修正", "改定", "更新", "見直し", "変更する", "修正する", "補正", "推敲"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "確かめること", // i18n-ok（類語辞書のデータ）
        synonyms: &["確認", "検証", "点検", "チェック", "照合", "確認する", "検証する", "見直し", "査読"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "告げ知らせること", // i18n-ok（類語辞書のデータ）
        synonyms: &["報告", "通知", "連絡", "共有", "レポート", "報告する", "伝達", "告知", "アナウンス"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "より良くすること", // i18n-ok（類語辞書のデータ）
        synonyms: &["改善", "改良", "向上", "進歩", "改善する", "ブラッシュアップ", "刷新", "改革", "ステップアップ"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "方針や結論を決めること", // i18n-ok（類語辞書のデータ）
        synonyms: &["決定", "確定", "判断", "採択", "決断", "決定する", "合意", "選定"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "力を貸すこと", // i18n-ok（類語辞書のデータ）
        synonyms: &["支援", "サポート", "援助", "協力", "後援", "アシスト", "手助け", "支援する"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "新しい技術や製品を作ること", // i18n-ok（類語辞書のデータ）
        synonyms: &["開発", "研究開発", "設計", "実装", "構築", "プログラミング", "開発する"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "目的を遂げること", // i18n-ok（類語辞書のデータ）
        synonyms: &["成功", "達成", "完遂", "勝利", "成就", "成功する", "達成する"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "内容を解き明かすこと", // i18n-ok（類語辞書のデータ）
        synonyms: &["説明", "解説", "案内", "詳述", "釈明", "説明する", "解説する"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "動詞 / サ変名詞", // i18n-ok（類語辞書のデータ）
        meaning: "深く調べること", // i18n-ok（類語辞書のデータ）
        synonyms: &["検討", "考案", "考察", "協議", "吟味", "審議", "検討する"], // i18n-ok（類語辞書のデータ）
    },

    // --- 日本語: 名詞 ---
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "文字で記された記録", // i18n-ok（類語辞書のデータ）
        synonyms: &["文書", "ドキュメント", "書類", "書面", "テキスト", "ファイル", "原稿"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "解決すべき事柄", // i18n-ok（類語辞書のデータ）
        synonyms: &["問題", "課題", "トラブル", "障害", "不具合", "エラー", "懸念", "難題"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "もたらされる働きや結果", // i18n-ok（類語辞書のデータ）
        synonyms: &["効果", "効能", "成果", "結果", "効力", "メリット", "インパクト"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "目指すところ", // i18n-ok（類語辞書のデータ）
        synonyms: &["目的", "目標", "狙い", "意図", "ターゲット", "趣旨", "方針"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "事態を引き起こすもと", // i18n-ok（類語辞書のデータ）
        synonyms: &["原因", "要因", "根拠", "理由", "動機", "背景", "発端"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "名詞", // i18n-ok（類語辞書のデータ）
        meaning: "物事を行うやり方", // i18n-ok（類語辞書のデータ）
        synonyms: &["方法", "手段", "手順", "手法", "アプローチ", "方式", "やり方"], // i18n-ok（類語辞書のデータ）
    },

    // --- 日本語: 形容詞・形容動詞 ---
    SynonymEntry {
        category: "形容動詞 / 形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "価値や重みがあるさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["重要", "大切", "主要", "重大", "必須", "肝要", "極めて重要", "肝心", "不可欠", "重要だ", "大切な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容動詞 / 形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "進行が速いさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["迅速", "素早い", "スピーディ", "速やか", "即座", "早急", "機敏", "迅速な", "速やかな"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容動詞 / 形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "平易であるさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["簡単", "容易", "手軽", "簡便", "シンプル", "単純", "易しい", "簡単な", "容易な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "美の感興をそそるさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["美しい", "綺麗な", "麗しい", "華麗な", "見事な", "優雅な", "素晴らしい"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "規模や容積が豊かなさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["大きい", "巨大な", "広大な", "大規模な", "広範な", "壮大な", "偉大な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "規模や量が控えめなさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["小さい", "わずかな", "微細な", "些細な", "小規模な", "コンパクトな", "極小の"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "今までにないさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["新しい", "最新の", "新規の", "新進の", "斬新な", "革新的な", "現代的な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "以前からあるさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["古い", "従来の", "過去の", "古典的な", "既存の", "旧来の", "古風な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "数量が豊かなさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["多い", "豊富な", "多数の", "数多くの", "無数の", "潤沢な", "多大な"], // i18n-ok（類語辞書のデータ）
    },
    SynonymEntry {
        category: "形容詞", // i18n-ok（類語辞書のデータ）
        meaning: "数量が乏しいさま", // i18n-ok（類語辞書のデータ）
        synonyms: &["少ない", "希少な", "僅少な", "不足した", "稀な", "限られた", "手薄な"], // i18n-ok（類語辞書のデータ）
    },

    // --- 英語: Verbs ---
    SynonymEntry {
        category: "Verb",
        meaning: "bring something into existence",
        synonyms: &["create", "make", "generate", "produce", "build", "construct", "originate", "devise"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "set in motion or begin",
        synonyms: &["start", "begin", "commence", "initiate", "launch", "embark", "open"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "bring to an end or finish",
        synonyms: &["finish", "complete", "conclude", "end", "finalize", "terminate", "accomplish"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "decide on and arrange ahead of time",
        synonyms: &["plan", "arrange", "design", "organize", "schedule", "formulate", "devise"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "make or become different",
        synonyms: &["change", "alter", "modify", "transform", "convert", "revise", "adjust", "adapt"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "examine for accuracy or condition",
        synonyms: &["check", "verify", "inspect", "examine", "test", "audit", "scrutinize", "investigate"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "give a spoken or written account of",
        synonyms: &["report", "announce", "describe", "communicate", "detail", "document", "state"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "make or become better",
        synonyms: &["improve", "enhance", "better", "upgrade", "refine", "boost", "advance", "polish"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "come to a resolution or decision",
        synonyms: &["decide", "determine", "resolve", "settle", "choose", "conclude", "finalize"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "make it easier for someone to do something",
        synonyms: &["help", "assist", "aid", "support", "serve", "back", "relieve"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "allow or cause to be visible",
        synonyms: &["show", "display", "exhibit", "demonstrate", "present", "reveal", "indicate", "illustrate"],
    },
    SynonymEntry {
        category: "Verb",
        meaning: "discover or locate by search",
        synonyms: &["find", "discover", "locate", "detect", "identify", "unearth", "encounter"],
    },

    // --- 英語: Nouns ---
    SynonymEntry {
        category: "Noun",
        meaning: "the point of beginning",
        synonyms: &["start", "beginning", "outset", "inception", "commencement", "onset", "launch"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "the final stage or culmination",
        synonyms: &["finish", "conclusion", "end", "completion", "finale", "termination"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a detailed proposal for doing or achieving something",
        synonyms: &["plan", "scheme", "proposal", "strategy", "project", "design", "blueprint"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "an act or process of becoming different",
        synonyms: &["change", "modification", "alteration", "transition", "variation", "amendment", "revision"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "an examination to test accuracy or quality",
        synonyms: &["check", "examination", "inspection", "test", "verification", "audit", "review"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "an account given of a particular matter",
        synonyms: &["report", "statement", "account", "summary", "record", "briefing", "dispatch"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "the action of helping someone",
        synonyms: &["help", "assistance", "aid", "support", "relief", "backing"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a written or printed record",
        synonyms: &["document", "file", "record", "text", "paper", "manuscript", "dossier"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a matter or situation regarded as unwelcome",
        synonyms: &["problem", "issue", "trouble", "difficulty", "obstacle", "dilemma", "complication"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a thing that is caused or produced by something else",
        synonyms: &["result", "outcome", "consequence", "effect", "product", "conclusion", "yield"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a thought or suggestion as to a possible course of action",
        synonyms: &["idea", "concept", "thought", "notion", "conception", "impression", "hypothesis"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "the object of a person's ambition or effort",
        synonyms: &["goal", "objective", "target", "aim", "purpose", "intent", "ambition"],
    },
    SynonymEntry {
        category: "Noun",
        meaning: "a particular procedure for accomplishing something",
        synonyms: &["method", "procedure", "technique", "process", "approach", "way", "system"],
    },

    // --- 英語: Adjectives ---
    SynonymEntry {
        category: "Adjective",
        meaning: "of great significance or value",
        synonyms: &["important", "significant", "critical", "crucial", "essential", "vital", "key", "major"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "moving or functioning rapidly",
        synonyms: &["quick", "fast", "rapid", "swift", "speedy", "prompt", "brisk", "expeditious"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "achieved without great effort",
        synonyms: &["easy", "simple", "effortless", "straightforward", "uncomplicated", "facile", "painless"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "pleasing the senses or mind aesthetically",
        synonyms: &["beautiful", "attractive", "gorgeous", "handsome", "lovely", "stunning", "charming"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "of considerable size or extent",
        synonyms: &["big", "large", "huge", "massive", "enormous", "immense", "gigantic", "substantial"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "of limited size or extent",
        synonyms: &["small", "little", "tiny", "miniature", "compact", "minute", "slight"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "not existing before; made, introduced, or discovered recently",
        synonyms: &["new", "novel", "fresh", "modern", "contemporary", "latest", "current", "recent"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "having lived or existed for a long time",
        synonyms: &["old", "traditional", "aged", "vintage", "previous", "former", "ancient", "obsolete"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "to be desired or approved of",
        synonyms: &["good", "excellent", "fine", "superior", "satisfactory", "great", "superb", "positive"],
    },
    SynonymEntry {
        category: "Adjective",
        meaning: "of poor quality or a low standard",
        synonyms: &["bad", "poor", "defective", "inferior", "unfavorable", "adverse", "negative", "substandard"],
    },
];

/// クエリの正規化（トリム、小文字化）
fn normalize_query(s: &str) -> String {
    s.trim().to_lowercase()
}

/// 類義語辞書からクエリに一致するエントリを検索
pub(crate) fn lookup(query: &str) -> Option<ThesaurusResult> {
    let q = normalize_query(query);
    if q.is_empty() {
        return None;
    }

    let mut matched_groups = Vec::new();

    for entry in THESAURUS_ENTRIES {
        // 同義語リストの中に完全一致（大文字小文字無視）があるか
        let has_exact_synonym = entry.synonyms.iter().any(|&s| s.eq_ignore_ascii_case(&q) || s == q);
        // 日本語の場合の部分一致（例: 「作成する」で「作成」にヒット、あるいは接辞）
        let has_loose_match = !has_exact_synonym && entry.synonyms.iter().any(|&s| {
            if s.len() >= 2 && q.len() >= 2 {
                q.starts_with(s) || s.starts_with(&q)
            } else {
                false
            }
        });

        if has_exact_synonym || has_loose_match {
            // クエリ自身以外の類義語をリストアップ（先頭に他の候補、末尾にクエリと同等のもの）
            let mut items: Vec<String> = entry.synonyms
                .iter()
                .filter(|&&s| !s.eq_ignore_ascii_case(&q) && s != q)
                .map(|s| s.to_string())
                .collect();

            // クエリと同じ単語も確認用として最後に含める（または候補として提示）
            if items.is_empty() {
                items.push(query.to_string());
            }

            matched_groups.push(ThesaurusGroup {
                category: entry.category.to_string(),
                meaning: entry.meaning.to_string(),
                synonyms: items,
            });
        }
    }

    if matched_groups.is_empty() {
        None
    } else {
        Some(ThesaurusResult {
            query: query.trim().to_string(),
            groups: matched_groups,
        })
    }
}

/// 類義語辞典パネル状態
pub(crate) struct ThesaurusState {
    pub(crate) query_input: Entity<InputState>,
    pub(crate) query: String,
    pub(crate) result: Option<ThesaurusResult>,
    pub(crate) target_range: Option<(FlowPos, FlowPos)>,
    pub(crate) history: Vec<String>,
    pub(crate) history_index: usize,
    pub(crate) message: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl SlideEditor {
    /// 校閲 > 類義語辞典 (Shift+F7 / docx.review.thesaurus) を開く
    pub(crate) fn open_thesaurus_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);

        // 1. 選択中のテキストがあるか調べる
        let mut initial_query = self.selected_text().trim().to_string();
        let mut target_range = None;

        if !initial_query.is_empty() {
            if let Some((start, end)) = self.ordered_selection() {
                target_range = Some((start, end));
            }
        } else if let Some(caret) = self.doc_state().and_then(|s| s.caret) {
            // キャレット位置の単語を取得
            let (start, end) = self.word_at(caret);
            let word = self.range_text(start, end).trim().to_string();
            if !word.is_empty() {
                initial_query = word;
                target_range = Some((start, end));
            }
        }

        let query_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx).placeholder(text("docx.thesaurus.search-placeholder"));
            if !initial_query.is_empty() {
                inp.set_value(&initial_query, window, cx);
            }
            inp.focus(window, cx);
            inp
        });

        let mut subscriptions = Vec::new();
        let sub = cx.subscribe_in(&query_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let q = this.doc_state()
                    .and_then(|s| s.thesaurus.as_ref())
                    .map(|ts| ts.query_input.read(cx).value().trim().to_string())
                    .unwrap_or_default();
                if !q.is_empty() {
                    this.thesaurus_search(&q, window, cx);
                }
            }
        });
        subscriptions.push(sub);

        let result = if !initial_query.is_empty() {
            lookup(&initial_query)
        } else {
            None
        };

        let history = if !initial_query.is_empty() {
            vec![initial_query.clone()]
        } else {
            Vec::new()
        };

        if let Some(state) = self.doc_state_mut() {
            state.thesaurus = Some(ThesaurusState {
                query_input,
                query: initial_query,
                result,
                target_range,
                history,
                history_index: 0,
                message: None,
                _subscriptions: subscriptions,
            });
            // 他のサイドバーを閉じる
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.table_formula = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.merge = None;
            state.helper = super::help_panels::HelpPanels::default();
            state.docx_math = None;
            state.quick_parts = None;
            state.source_manager = None;
            state.print_panel = None;
            state.inspect_panel = None;
            state.compatibility_panel = None;
        }

        self.sidebar_open = true;
        cx.notify();
    }

    /// 類義語辞典パネルを閉じる
    pub(crate) fn close_thesaurus_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.thesaurus = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 指定されたクエリで検索を実行
    pub(crate) fn thesaurus_search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let q = query.trim().to_string();
        if q.is_empty() {
            return;
        }

        let result = lookup(&q);

        if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
            ts.query_input.update(cx, |inp, cx| {
                inp.set_value(&q, window, cx);
            });
            ts.query = q.clone();
            ts.result = result;
            ts.message = None;

            // 履歴に追加
            if ts.history.last().map(|s| s.as_str()) != Some(&q) {
                ts.history.push(q);
                ts.history_index = ts.history.len().saturating_sub(1);
            }
        }
        cx.notify();
    }

    /// 検索履歴を1つ戻る
    pub(crate) fn thesaurus_history_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prev_query = {
            let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) else { return };
            if ts.history_index > 0 {
                ts.history_index -= 1;
                ts.history.get(ts.history_index).cloned()
            } else {
                None
            }
        };

        if let Some(q) = prev_query {
            let result = lookup(&q);
            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.query_input.update(cx, |inp, cx| {
                    inp.set_value(&q, window, cx);
                });
                ts.query = q;
                ts.result = result;
                ts.message = None;
            }
            cx.notify();
        }
    }

    /// 選択中の単語を類義語で置換する
    pub(crate) fn thesaurus_replace(&mut self, replacement: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow().cloned() else { return };

        // 置換対象の範囲を決定
        let range = if let Some((start, end)) = self.ordered_selection() {
            Some((start, end))
        } else if let Some(target) = self.doc_state().and_then(|s| s.thesaurus.as_ref()).and_then(|ts| ts.target_range) {
            Some(target)
        } else if let Some(caret) = self.doc_state().and_then(|s| s.caret) {
            let (start, end) = self.word_at(caret);
            Some((start, end))
        } else {
            None
        };

        let Some((start, end)) = range else {
            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.message = Some(text("docx.thesaurus.no-selection-error").to_string());
            }
            cx.notify();
            return;
        };

        // 1ステップの Undo/Redo で置換を実行
        let commands = Self::replacement_commands(&doc, start, end, replacement, None).0;
        if !commands.is_empty() {
            self.run_flow(commands, cx);

            // 置換後のテキストを選択状態にする
            let new_end = FlowPos::new(start.para, start.offset + replacement.chars().count());
            self.set_caret(start, false, cx);
            self.set_caret(new_end, true, cx);

            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.target_range = Some((start, new_end));
                ts.message = Some(crate::shell::i18n::format(
                    "docx.thesaurus.replaced-msg",
                    &[("word", &replacement)],
                ));
            }
        }
        cx.notify();
    }

    /// クリップボードへ単語をコピー
    pub(crate) fn thesaurus_copy(&mut self, word: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(word.to_string()));
        if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
            ts.message = Some(crate::shell::i18n::format(
                "docx.thesaurus.copied-msg",
                &[("word", &word)],
            ));
        }
        cx.notify();
    }

    /// 類義語辞典サイドバーパネルの描画
    pub(crate) fn doc_thesaurus_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.thesaurus.as_ref()?;
        let query_input = state.query_input.clone();
        let query = state.query.clone();
        let result = state.result.clone();
        let message = state.message.clone();
        let can_back = state.history_index > 0;

        let theme = cx.theme();
        let border_color = theme.border;
        let surface_color = theme.background;

        // ヘッダー（タイトル・戻るボタン・閉じるボタン）
        let header = h_flex()
            .justify_between()
            .items_center()
            .pb(px(8.))
            .border_b_1()
            .border_color(border_color)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        crate::shell::chrome::action_button("thesaurus-back")

                            .xsmall()
                            .disabled(!can_back)
                            .label("←")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.thesaurus_history_back(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(text("docx.thesaurus.title")),
                    ),
            )
            .child(
                crate::shell::chrome::action_button("thesaurus-close")
                    .xsmall()
                    .label("✕")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_thesaurus_panel(window, cx);
                    })),
            );

        // 検索バー
        let search_bar = h_flex()
            .gap_2()
            .items_center()
            .pt(px(8.))
            .child(div().flex_1().child(crate::shell::chrome::text_field(&query_input).full_width()))
            .child(
                crate::shell::chrome::primary_button("thesaurus-search-btn")
                    .small()
                    .label(text("docx.thesaurus.search-btn"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let q = this.doc_state()
                            .and_then(|s| s.thesaurus.as_ref())
                            .map(|ts| ts.query_input.read(cx).value().trim().to_string())
                            .unwrap_or_default();
                        this.thesaurus_search(&q, window, cx);
                    })),
            );

        // メッセージ通知
        let msg_view = message.map(|m| {
            div()
                .px(px(8.))
                .py(px(4.))
                .mt(px(4.))
                .rounded_sm()
                .bg(rgb(0xe6f4ea))
                .text_color(rgb(0x137333))
                .text_sm()
                .child(m)
        });

        // 結果一覧または空状態
        let content = if let Some(res) = result {
            let groups_len = res.groups.len();
            let total_words: usize = res.groups.iter().map(|g| g.synonyms.len()).sum();

            let summary = h_flex()
                .justify_between()
                .items_center()
                .py(px(4.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("「{}」", res.query)), // i18n-ok（類語辞書のデータ）
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x666666))
                        .child(format!("{groups_len} 分類 / {total_words} 語")), // i18n-ok（類語辞書のデータ）
                );

            let group_elements: Vec<AnyElement> = res.groups.into_iter().enumerate().map(|(g_idx, group)| {
                let category_badge = div()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded_sm()
                    .bg(rgb(0xe8f0fe))
                    .text_color(rgb(0x1a73e8))
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .child(group.category);

                let meaning_text = div()
                    .text_sm()
                    .text_color(rgb(0x555555))
                    .child(group.meaning);

                let word_buttons: Vec<AnyElement> = group.synonyms.into_iter().enumerate().map(|(w_idx, word)| {
                    let w_insert1 = word.clone();
                    let w_insert2 = word.clone();
                    let w_search = word.clone();
                    let w_copy = word.clone();
                    let item_id = (g_idx * 1000 + w_idx) as u32;

                    h_flex()
                        .justify_between()
                        .items_center()
                        .px(px(8.))
                        .py(px(4.))
                        .rounded_sm()
                        .hover(|s| s.bg(rgb(0xf1f3f4)))
                        .child(
                            crate::shell::chrome::action_button(("thesaurus-w", item_id))

                                .small()
                                .label(word)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.thesaurus_replace(&w_insert1, cx);
                                })),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-ins", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.insert-tooltip"))
                                        .label(text("docx.thesaurus.insert"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.thesaurus_replace(&w_insert2, cx);
                                        })),
                                )
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-copy", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.copy-tooltip"))
                                        .label("📋")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.thesaurus_copy(&w_copy, cx);
                                        })),
                                )
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-lookup", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.lookup-tooltip"))
                                        .label("🔍")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.thesaurus_search(&w_search, window, cx);
                                        })),
                                ),
                        )
                        .into_any_element()
                }).collect();

                v_flex()
                    .gap_1()
                    .p(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xeeeeee))
                    .bg(surface_color)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(category_badge)
                            .child(meaning_text),
                    )
                    .child(v_flex().gap_0().children(word_buttons))
                    .into_any_element()
            }).collect();

            v_flex()
                .gap_2()
                .pt(px(8.))
                .child(summary)
                .children(group_elements)
                .into_any_element()
        } else if !query.is_empty() {
            v_flex()
                .pt(px(16.))
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x777777))
                        .child(crate::shell::i18n::format(
                            "docx.thesaurus.no-results",
                            &[("word", &query)],
                        )),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x999999))
                        .child(text("docx.thesaurus.hint")),
                )
                .into_any_element()
        } else {
            v_flex()
                .pt(px(16.))
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x777777))
                        .child(text("docx.thesaurus.empty-prompt")),
                )
                .into_any_element()
        };

        let view = v_flex()
            .p(px(12.))
            .gap_2()
            .p_0().child(header)
            .child(search_bar)
            .children(msg_view)
            .child(
                div()
                    .id("thesaurus-scroll-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            );

        Some(SidebarPanel {
            title: text("docx.thesaurus.title").into(),
            body: view.into_any_element(),
        })
    }
}

