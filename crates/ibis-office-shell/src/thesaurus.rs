// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 類義語辞典 (Thesaurus: DOCX-REVIEW-019) の辞書と検索（文書画面・表計算画面・プレゼンテーション画面の類義語パネルが共有する。
//! 文書画面のパネルの状態と操作は docx/thesaurus.rs）:
//! Word 標準の校閲 > 類義語辞典 (Thesaurus / Shift+F7 / docx.review.thesaurus)。
//! - 選択中テキストまたはキャレット位置の単語を自動検出して検索
//! - 品詞別・意味別に分類された類義語・同義語リストの表示
//! - ワンクリックでの本文への置換 (1ステップの Undo/Redo 連動)
//! - クリップボードへのコピー
//! - 検索履歴と前の検索結果への戻る操作
//! - 日英の豊富なビジネス・学術・日常語彙を内包（クリーンルーム実装）


/// 1件のシソーラスグループ定義（品詞、意味、類語配列）
#[derive(Clone, Debug)]
pub struct SynonymEntry {
    pub(crate) category: &'static str,
    pub(crate) meaning: &'static str,
    pub(crate) synonyms: &'static [&'static str],
}

/// 検索結果の1グループ
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThesaurusGroup {
    pub category: String,
    pub meaning: String,
    pub synonyms: Vec<String>,
}

/// 類義語検索結果
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThesaurusResult {
    pub query: String,
    pub groups: Vec<ThesaurusGroup>,
}

/// クリーンルーム実装による内包シソーラス辞書データ
pub const THESAURUS_ENTRIES: &[SynonymEntry] = &[
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
pub fn lookup(query: &str) -> Option<ThesaurusResult> {
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
