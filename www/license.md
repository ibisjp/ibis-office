# サードパーティのライセンス（Web 版）

ibisOffice の Web 版（`www/`）に含まれる、ibisOffice 以外の部品と、そのライセンスが求める表示です。このファイルは `scripts/gen-web-licenses.py` で生成しています。

## 同梱しているデータ・フォント・アイコン

| 部品 | 用途 | ライセンス |
|---|---|---|
| ECMA-376 presetShapeDefinitions.xml | 図形（プリセット図形）の輪郭の定義 | Ecma テキスト著作権ポリシー |
| ECMA-376 presetTableStyles.xml | Excel の組み込みテーブル スタイルの定義 | Ecma テキスト著作権ポリシー |
| ECMA-376 presetCellStyles.xml | Excel の組み込みセルのスタイルの定義 | Ecma テキスト著作権ポリシー |
| ECMA-376 OfficeOpenXML-WordprocessingMLArtBorders.zip | Word のページの絵柄罫線の絵（`assets/artborders/` に PNG を 1 枚ずつ、改変せずに） | Ecma テキスト著作権ポリシー |
| IBM Plex Sans | UI の文字（Web 版） | SIL Open Font License 1.1 |
| Lucide | アイコン（`assets/icons/`） | ISC |
| Material Symbols（Google） | タイトル行・メニュー・ツールバー・サイドバーのアイコン（実行ファイルに埋め込み） | Apache-2.0 |

## Rust の部品（409 件）

| 名前 | 版 | ライセンス | 入手先 |
|---|---|---|---|
| accesskit | 0.24.1 | MIT OR Apache-2.0 | https://github.com/AccessKit/accesskit |
| addr2line | 0.25.1 | Apache-2.0 OR MIT | https://github.com/gimli-rs/addr2line |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | https://github.com/oyvindln/adler2 |
| aes | 0.8.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/block-ciphers |
| aho-corasick | 1.1.5 | Unlicense OR MIT | https://github.com/BurntSushi/aho-corasick |
| annotate-snippets | 0.12.16 | MIT OR Apache-2.0 | https://github.com/rust-lang/annotate-snippets-rs |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | https://github.com/dtolnay/anyhow |
| arc-swap | 1.9.2 | MIT OR Apache-2.0 | https://github.com/vorner/arc-swap |
| arraydeque | 0.5.1 | MIT/Apache-2.0 | https://github.com/andylokandy/arraydeque |
| arrayref | 0.3.9 | BSD-2-Clause | https://github.com/droundy/arrayref |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | https://github.com/bluss/arrayvec |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-channel |
| async-compression | 0.4.48 | MIT OR Apache-2.0 | https://github.com/Nullus157/async-compression |
| async-task | 4.7.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/async-task |
| atomic | 0.5.3 | Apache-2.0/MIT | https://github.com/Amanieu/atomic-rs |
| backtrace | 0.3.76 | MIT OR Apache-2.0 | https://github.com/rust-lang/backtrace-rs |
| base62 | 2.2.6 | MIT | https://github.com/fbernier/base62 |
| base64 | 0.22.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| base64 | 0.23.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| bit-set | 0.9.1 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-set |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-vec |
| bit_field | 0.10.3 | Apache-2.0/MIT | https://github.com/phil-opp/rust-bit-field |
| bitflags | 1.3.2 | MIT/Apache-2.0 | https://github.com/bitflags/bitflags |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | https://github.com/bitflags/bitflags |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| block-padding | 0.3.3 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| bstr | 1.13.1 | MIT OR Apache-2.0 | https://github.com/BurntSushi/bstr |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | https://github.com/fitzgen/bumpalo |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/bytemuck |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/bytemuck |
| byteorder | 1.5.0 | Unlicense OR MIT | https://github.com/BurntSushi/byteorder |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | https://github.com/image-rs/byteorder-lite |
| bytes | 1.12.1 | MIT | https://github.com/tokio-rs/bytes |
| bzip2 | 0.6.1 | MIT OR Apache-2.0 | https://github.com/trifectatechfoundation/bzip2-rs |
| cbc | 0.1.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/block-modes |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| chrono | 0.4.45 | MIT OR Apache-2.0 | https://github.com/chronotope/chrono |
| cipher | 0.4.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| codespan-reporting | 0.13.1 | Apache-2.0 | https://github.com/brendanzab/codespan |
| color_quant | 1.1.0 | MIT | https://github.com/image-rs/color_quant.git |
| compression-codecs | 0.4.43 | MIT OR Apache-2.0 | https://github.com/Nullus157/async-compression |
| compression-core | 0.4.33 | MIT OR Apache-2.0 | https://github.com/Nullus157/async-compression |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/concurrent-queue |
| console_error_panic_hook | 0.1.7 | Apache-2.0/MIT | https://github.com/rustwasm/console_error_panic_hook |
| convert_case | 0.10.0 | MIT | https://github.com/rutrum/convert-case |
| core_maths | 0.1.1 | MIT | https://github.com/robertbastian/core_maths |
| cosmic-text | 0.19.0 | MIT OR Apache-2.0 | https://github.com/pop-os/cosmic-text |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | https://github.com/srijs/rust-crc32fast |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| ctor | 1.0.13 | Apache-2.0 OR MIT | https://github.com/mmastrac/linktime |
| data-url | 0.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| derive_more | 2.1.1 | MIT | https://github.com/JelteF/derive_more |
| derive_more-impl | 2.1.1 | MIT | https://github.com/JelteF/derive_more |
| digest | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | https://github.com/yaahc/displaydoc |
| document-features | 0.2.12 | MIT OR Apache-2.0 | https://github.com/slint-ui/document-features |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | https://github.com/dtolnay/dyn-clone |
| either | 1.18.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/either |
| encoding_rs | 0.8.41 | (Apache-2.0 OR MIT) AND BSD-3-Clause | https://github.com/hsivonen/encoding_rs |
| encoding_rs_io | 0.1.8 | MIT OR Apache-2.0 | https://github.com/BurntSushi/encoding_rs_io |
| enum-iterator | 2.3.0 | 0BSD | https://github.com/stephaneyfx/enum-iterator.git |
| enum-iterator-derive | 1.5.0 | 0BSD | https://github.com/stephaneyfx/enum-iterator.git |
| enumn | 0.1.14 | MIT OR Apache-2.0 | https://github.com/dtolnay/enumn |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/equivalent |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 | https://github.com/dtolnay/erased-serde |
| etagere | 0.2.15 | MIT/Apache-2.0 | https://github.com/nical/etagere |
| euclid | 0.22.14 | MIT OR Apache-2.0 | https://github.com/servo/euclid |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/event-listener |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | https://github.com/smol-rs/event-listener-strategy |
| exr | 1.74.2 | BSD-3-Clause | https://github.com/johannesvollmer/exrs |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/fastrand |
| fax | 0.2.7 | MIT | https://github.com/pdf-rs/fax |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | https://github.com/image-rs/fdeflate |
| filetime | 0.2.29 | MIT/Apache-2.0 | https://github.com/alexcrichton/filetime |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | https://github.com/petgraph/fixedbitset |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | https://github.com/rust-lang/flate2-rs |
| float-cmp | 0.9.0 | MIT | https://github.com/mikedilger/float-cmp |
| float_next_after | 1.0.0 | MIT | https://gitlab.com/bronsonbdevost/next_afterf |
| fluent-uri | 0.1.4 | MIT | https://github.com/yescallop/fluent-uri-rs |
| flume | 0.12.0 | Apache-2.0/MIT | https://github.com/zesterer/flume |
| foldhash | 0.2.0 | Zlib | https://github.com/orlp/foldhash |
| font-types | 0.11.3 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| font-types | 0.12.5 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| fontdb | 0.23.0 | MIT | https://github.com/RazrFalcon/fontdb |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| futf | 0.1.5 | MIT / Apache-2.0 | https://github.com/servo/futf |
| futures | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-concurrency | 7.7.1 | MIT OR Apache-2.0 | https://github.com/yoshuawuyts/futures-concurrency |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/futures-lite |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| generic-array | 0.14.7 | MIT | https://github.com/fizyk20/generic-array.git |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| gif | 0.14.2 | MIT OR Apache-2.0 | https://github.com/image-rs/image-gif |
| gimli | 0.32.3 | MIT OR Apache-2.0 | https://github.com/gimli-rs/gimli |
| glob | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/glob |
| globset | 0.4.20 | Unlicense OR MIT | https://github.com/BurntSushi/ripgrep/tree/master/crates/globset |
| globwalk | 0.8.1 | MIT | https://github.com/gilnaa/globwalk |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | https://github.com/grovesNL/glow |
| gpui-base | 0.6.6 | Apache-2.0 | https://github.com/longbridge/gpui-kit |
| gpui-component | 0.6.6 | Apache-2.0 | https://github.com/longbridge/gpui-kit |
| gpui-component-macros | 0.6.6 | Apache-2.0 | https://crates.io/crates/gpui-component-macros |
| gpui-kit | 0.6.6 | Apache-2.0 | https://github.com/longbridge/gpui-kit |
| gpui-kit-assets | 0.6.6 | Apache-2.0 | https://github.com/longbridge/gpui-kit |
| gpui-pre | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-collections | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-derive-refineable | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-http-client | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-macros | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-perf | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-platform | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-refineable | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-reqwest | 0.12.15 | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| gpui-pre-scheduler | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-shared-string | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-sum-tree | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-util | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-util-macros | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-web | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-wgpu | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-zlog | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-ztracing | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| gpui-pre-ztracing-macro | 0.3.6 | Apache-2.0 | https://github.com/zed-industries/zed |
| granit-parser | 1.3.0 | MIT OR Apache-2.0 | https://github.com/bourumir-wyngs/granit-parser |
| half | 2.7.1 | MIT OR Apache-2.0 | https://github.com/VoidStarKat/half-rs |
| harfrust | 0.5.2 | MIT | https://github.com/harfbuzz/harfrust |
| hash32 | 0.3.1 | MIT OR Apache-2.0 | https://github.com/japaric/hash32 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| heapless | 0.9.3 | MIT OR Apache-2.0 | https://github.com/rust-embedded/heapless |
| heck | 0.5.0 | MIT OR Apache-2.0 | https://github.com/withoutboats/heck |
| hexf-parse | 0.2.1 | CC0-1.0 | https://github.com/lifthrasiir/hexf |
| hmac | 0.12.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/MACs |
| html5ever | 0.27.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| http | 1.5.0 | MIT OR Apache-2.0 | https://github.com/hyperium/http |
| http-body | 1.1.0 | MIT | https://github.com/hyperium/http-body |
| ibis-calc | 0.8.0 | AGPL-3.0-only | https://crates.io/crates/ibis-calc |
| ibis-ooxml | 0.8.0 | AGPL-3.0-only | https://crates.io/crates/ibis-ooxml |
| icu_collections | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_core | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties_data | 2.3.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_provider | 2.3.1 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| idna | 1.1.0 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | https://github.com/hsivonen/idna_adapter |
| ignore | 0.4.33 | Unlicense OR MIT | https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore |
| image | 0.25.10 | MIT OR Apache-2.0 | https://github.com/image-rs/image |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | https://github.com/image-rs/image-webp |
| imagesize | 0.14.0 | MIT | https://github.com/Roughsketch/imagesize |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/indexmap |
| inout | 0.1.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| instant | 0.1.13 | BSD-3-Clause | https://github.com/sebcrozet/instant |
| inventory | 0.3.24 | MIT OR Apache-2.0 | https://github.com/dtolnay/inventory |
| itertools | 0.11.0 | MIT OR Apache-2.0 | https://github.com/rust-itertools/itertools |
| itertools | 0.13.0 | MIT OR Apache-2.0 | https://github.com/rust-itertools/itertools |
| itertools | 0.14.0 | MIT OR Apache-2.0 | https://github.com/rust-itertools/itertools |
| itoa | 1.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/itoa |
| js-sys | 0.3.105 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | https://github.com/linebender/kurbo |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 | https://github.com/rust-lang-nursery/lazy-static.rs |
| lebe | 0.5.3 | BSD-3-Clause | https://github.com/johannesvollmer/lebe |
| libbz2-rs-sys | 0.2.5 | bzip2-1.0.6 | https://github.com/trifectatechfoundation/libbzip2-rs |
| libc | 0.2.189 | MIT OR Apache-2.0 | https://github.com/rust-lang/libc |
| libm | 0.2.16 | MIT | https://github.com/rust-lang/compiler-builtins |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | https://github.com/linebender/raw_resource_handle |
| link-section | 0.19.3 | Apache-2.0 OR MIT | https://github.com/mmastrac/linktime |
| linktime-proc-macro | 0.2.3 | Apache-2.0 OR MIT | https://github.com/mmastrac/linktime |
| litemap | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| litrs | 1.0.0 | MIT OR Apache-2.0 | https://github.com/LukasKalbertodt/litrs |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| log | 0.4.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/log |
| lsp-types | 0.97.0 | MIT | https://github.com/gluon-lang/lsp-types |
| lyon | 1.0.19 | MIT OR Apache-2.0 | https://github.com/nical/lyon |
| lyon_algorithms | 1.0.21 | MIT OR Apache-2.0 | https://github.com/nical/lyon |
| lyon_geom | 1.0.19 | MIT OR Apache-2.0 | https://github.com/nical/lyon |
| lyon_path | 1.0.19 | MIT OR Apache-2.0 | https://github.com/nical/lyon |
| lyon_tessellation | 1.0.22 | MIT OR Apache-2.0 | https://github.com/nical/lyon |
| mac | 0.1.1 | MIT/Apache-2.0 | https://github.com/reem/rust-mac.git |
| markdown | 1.0.0 | MIT | https://github.com/wooorm/markdown-rs |
| markup5ever | 0.12.1 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| markup5ever_rcdom | 0.3.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| md-5 | 0.10.6 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| memchr | 2.8.3 | Unlicense OR MIT | https://github.com/BurntSushi/memchr |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | https://github.com/RazrFalcon/memmap2-rs |
| mime | 0.3.17 | MIT OR Apache-2.0 | https://github.com/hyperium/mime |
| mime_guess | 2.0.5 | MIT | https://github.com/abonander/mime_guess |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | https://github.com/awxkee/moxcms.git |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | https://github.com/hsivonen/multiversion_no_op |
| naga | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| new_debug_unreachable | 1.0.6 | MIT | https://github.com/mbrubeck/rust-debug-unreachable |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | https://github.com/paritytech/nohash-hasher |
| normpath | 1.5.2 | MIT OR Apache-2.0 | https://github.com/dylni/normpath |
| notify | 7.0.0 | CC0-1.0 | https://github.com/notify-rs/notify.git |
| notify-types | 1.0.1 | MIT OR Apache-2.0 | https://github.com/notify-rs/notify.git |
| nu-ansi-term | 0.50.3 | MIT | https://github.com/nushell/nu-ansi-term |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | https://github.com/rust-num/num-complex |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | https://github.com/rust-num/num-traits |
| num_cpus | 1.17.0 | MIT OR Apache-2.0 | https://github.com/seanmonstar/num_cpus |
| object | 0.37.3 | Apache-2.0 OR MIT | https://github.com/gimli-rs/object |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | https://github.com/matklad/once_cell |
| parking | 2.2.1 | Apache-2.0 OR MIT | https://github.com/smol-rs/parking |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| paste | 1.0.15 | MIT OR Apache-2.0 | https://github.com/dtolnay/paste |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| phf | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.11.3 | MIT | https://github.com/rust-phf/rust-phf |
| pico-args | 0.5.0 | MIT | https://github.com/RazrFalcon/pico-args |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project-lite |
| png | 0.17.16 | MIT OR Apache-2.0 | https://github.com/image-rs/image-png |
| png | 0.18.1 | MIT OR Apache-2.0 | https://github.com/image-rs/image-png |
| pollster | 0.2.5 | Apache-2.0/MIT | https://github.com/zesterer/pollster |
| pollster | 0.4.0 | Apache-2.0/MIT | https://github.com/zesterer/pollster |
| polycool | 0.4.0 | MIT OR Apache-2.0 | https://github.com/linebender/kurbo |
| postage | 0.5.0 | MIT | https://github.com/austinjones/postage-rs |
| potential_utf | 0.1.6 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | https://github.com/cryptocorrosion/cryptocorrosion |
| precomputed-hash | 0.1.1 | MIT | https://github.com/emilio/precomputed-hash |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | https://github.com/bkchr/proc-macro-crate |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | https://github.com/dtolnay/proc-macro2 |
| profiling | 1.0.18 | MIT OR Apache-2.0 | https://github.com/aclysma/profiling |
| profiling-procmacros | 1.0.18 | MIT OR Apache-2.0 | https://github.com/aclysma/profiling |
| pulp | 0.22.3 | MIT | https://github.com/sarah-quinones/pulp/ |
| pulp-wasm-simd-flag | 0.1.1 | MIT | https://github.com/sarah-quinones/pulp/ |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | https://github.com/awxkee/pxfm |
| qoi | 0.4.1 | MIT/Apache-2.0 | https://github.com/aldanor/qoi-rust |
| quick-error | 2.0.1 | MIT/Apache-2.0 | http://github.com/tailhook/quick-error |
| quote | 1.0.47 | MIT OR Apache-2.0 | https://github.com/dtolnay/quote |
| rand | 0.9.5 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rangemap | 1.8.0 | MIT/Apache-2.0 | https://github.com/jeffparsons/rangemap |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | https://github.com/rust-windowing/raw-window-handle |
| rayon | 1.12.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/rayon |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/rayon |
| read-fonts | 0.37.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| reborrow | 0.5.5 | MIT | https://github.com/sarah-ek/reborrow/ |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | https://github.com/dtolnay/ref-cast |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | https://github.com/dtolnay/ref-cast |
| regex | 1.13.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| resvg | 0.46.0 | Apache-2.0 OR MIT | https://github.com/linebender/resvg |
| rgb | 0.8.53 | MIT | https://github.com/kornelski/rust-rgb |
| ropey | 2.0.0-beta.1 | MIT OR Apache-2.0 | https://github.com/cessen/ropey |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 | https://github.com/RazrFalcon/roxmltree |
| rust-i18n | 4.2.2 | MIT | https://github.com/longbridge/rust-i18n |
| rust-i18n-macro | 4.2.2 | MIT | https://github.com/longbridge/rust-i18n |
| rust-i18n-support | 4.2.2 | MIT | https://github.com/longbridge/rust-i18n |
| rustc-demangle | 0.1.28 | MIT/Apache-2.0 | https://github.com/rust-lang/rustc-demangle |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | https://github.com/rust-lang-nursery/rustc-hash |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | https://github.com/rust-lang/rustc-hash |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | https://github.com/dtolnay/rustversion |
| rustybuzz | 0.20.1 | MIT | https://github.com/harfbuzz/rustybuzz |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://github.com/dtolnay/ryu |
| same-file | 1.0.6 | Unlicense/MIT | https://github.com/BurntSushi/same-file |
| schemars | 1.2.2 | MIT | https://github.com/GREsau/schemars |
| schemars_derive | 1.2.2 | MIT | https://github.com/GREsau/schemars |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | https://github.com/bluss/scopeguard |
| seahash | 4.1.0 | MIT | https://gitlab.redox-os.org/redox-os/seahash |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only | https://github.com/Voultapher/self_cell |
| serde | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde-saphyr | 1.3.0 | MIT OR Apache-2.0 | https://github.com/bourumir-wyngs/serde-saphyr |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_fmt | 1.1.0 | Apache-2.0 OR MIT | https://github.com/KodrAus/serde_fmt.git |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/serde-repr |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | https://github.com/nox/serde_urlencoded |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| sha1_smol | 1.0.1 | BSD-3-Clause | https://github.com/mitsuhiko/sha1-smol |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| sharded-slab | 0.1.7 | MIT | https://github.com/hawkw/sharded-slab |
| simd-adler32 | 0.3.10 | MIT | https://github.com/mcountryman/simd-adler32 |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | https://github.com/rusticstuff/simdutf8 |
| simplecss | 0.2.2 | Apache-2.0 OR MIT | https://github.com/linebender/simplecss |
| siphasher | 1.0.3 | MIT/Apache-2.0 | https://github.com/jedisct1/rust-siphash |
| skrifa | 0.40.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| slab | 0.4.12 | MIT | https://github.com/tokio-rs/slab |
| slotmap | 1.1.1 | Zlib | https://github.com/orlp/slotmap |
| smallvec | 1.16.1 | MIT OR Apache-2.0 | https://github.com/servo/rust-smallvec |
| smol_str | 0.3.6 | MIT OR Apache-2.0 | https://github.com/rust-lang/rust-analyzer/tree/master/lib/smol_str |
| spin | 0.10.1 | MIT | https://github.com/mvdnes/spin-rs.git |
| spin | 0.9.9 | MIT | https://github.com/mvdnes/spin-rs.git |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | https://github.com/storyyeller/stable_deref_trait |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | https://github.com/nvzqz/static-assertions-rs |
| str_indices | 0.4.4 | MIT OR Apache-2.0 | https://github.com/cessen/str_indices |
| strict-num | 0.1.1 | MIT | https://github.com/RazrFalcon/strict-num |
| string_cache | 0.8.9 | MIT OR Apache-2.0 | https://github.com/servo/string-cache |
| strum | 0.28.0 | MIT | https://github.com/Peternator7/strum |
| strum_macros | 0.28.0 | MIT | https://github.com/Peternator7/strum |
| subtle | 2.6.1 | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| svg_fmt | 0.4.5 | MIT/Apache-2.0 | https://github.com/nical/rust_debug |
| svgtypes | 0.16.1 | Apache-2.0 OR MIT | https://github.com/linebender/svgtypes |
| swash | 0.2.10 | Apache-2.0 OR MIT | https://github.com/dfrg/swash |
| syn | 2.0.119 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| syn | 3.0.6 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| sync_wrapper | 1.0.2 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| synstructure | 0.14.0 | MIT | https://github.com/mystor/synstructure |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 | https://github.com/1Password/sys-locale |
| taffy | 0.13.0 | MIT | https://github.com/DioxusLabs/taffy |
| tendril | 0.4.3 | MIT/Apache-2.0 | https://github.com/servo/tendril |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | https://github.com/Amanieu/thread_local-rs |
| tiff | 0.11.3 | MIT | https://github.com/image-rs/image-tiff |
| tiny-skia | 0.11.4 | BSD-3-Clause | https://github.com/RazrFalcon/tiny-skia |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | https://github.com/RazrFalcon/tiny-skia/tree/master/path |
| tinystr | 0.8.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/tinyvec |
| toml | 0.8.23 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_datetime | 0.6.11 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_edit | 0.22.27 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_write | 0.1.2 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| tower-service | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| tracing | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| tracing-attributes | 0.1.31 | MIT | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | MIT | https://github.com/tokio-rs/tracing |
| tracing-log | 0.2.0 | MIT | https://github.com/tokio-rs/tracing |
| tracing-subscriber | 0.3.23 | MIT | https://github.com/tokio-rs/tracing |
| triomphe | 0.1.16 | MIT OR Apache-2.0 | https://github.com/Manishearth/triomphe |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | https://github.com/harfbuzz/ttf-parser |
| typeid | 1.0.3 | MIT OR Apache-2.0 | https://github.com/dtolnay/typeid |
| typenum | 1.20.1 | MIT OR Apache-2.0 | https://github.com/paholg/typenum |
| unicase | 2.9.0 | MIT OR Apache-2.0 | https://github.com/seanmonstar/unicase |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | https://github.com/servo/unicode-bidi |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 | https://github.com/RazrFalcon/unicode-bidi-mirroring |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 | https://github.com/RazrFalcon/unicode-ccc |
| unicode-id | 0.3.7 | MIT OR Apache-2.0 | https://github.com/Boshen/unicode-id |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| unicode-linebreak | 0.1.5 | Apache-2.0 | https://github.com/axelf4/unicode-linebreak |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 | https://github.com/unicode-rs/unicode-properties |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-script |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-segmentation |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 | https://github.com/RazrFalcon/unicode-vo |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-width |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-xid |
| url | 2.5.8 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| usvg | 0.46.0 | Apache-2.0 OR MIT | https://github.com/linebender/resvg |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 | https://github.com/SimonSapin/rust-utf8 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | https://github.com/hsivonen/utf8_iter |
| uuid | 1.26.1 | Apache-2.0 OR MIT | https://github.com/uuid-rs/uuid |
| value-bag | 1.14.1 | Apache-2.0 OR MIT | https://github.com/sval-rs/value-bag |
| value-bag-serde1 | 1.14.1 | Apache-2.0 OR MIT | https://crates.io/crates/value-bag-serde1 |
| waker-fn | 1.2.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/waker-fn |
| walkdir | 2.5.0 | Unlicense/MIT | https://github.com/BurntSushi/walkdir |
| wasm-bindgen | 0.2.128 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen |
| wasm-bindgen-futures | 0.4.78 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures |
| wasm-bindgen-macro | 0.2.128 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| wasm-bindgen-macro-support | 0.2.128 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support |
| wasm-bindgen-shared | 0.2.128 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| wasm-streams | 0.4.2 | MIT OR Apache-2.0 | https://github.com/MattiasBuelens/wasm-streams/ |
| wasm_thread | 0.3.3 | Apache-2.0 OR MIT | https://github.com/chemicstry/wasm_thread |
| web-sys | 0.3.105 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| web-time | 1.1.0 | MIT OR Apache-2.0 | https://github.com/daxpedda/web-time |
| weezl | 0.1.12 | MIT OR Apache-2.0 | https://github.com/image-rs/weezl |
| wgpu | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| wgpu-core | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| wgpu-core-deps-wasm | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| wgpu-hal | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| wgpu-naga-bridge | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| wgpu-types | 29.0.4 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| winnow | 0.7.15 | MIT | https://github.com/winnow-rs/winnow |
| winnow | 1.0.4 | MIT | https://github.com/winnow-rs/winnow |
| writeable | 0.6.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| xml5ever | 0.18.1 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| xmlwriter | 0.1.0 | MIT | https://github.com/RazrFalcon/xmlwriter |
| yazi | 0.2.1 | Apache-2.0 OR MIT | https://github.com/dfrg/yazi |
| yoke | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| yoke-derive | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zeno | 0.3.3 | Apache-2.0 OR MIT | https://github.com/dfrg/zeno |
| zerocopy | 0.8.58 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| zerocopy-derive | 0.8.58 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| zerofrom | 0.1.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerotrie | 0.2.5 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec | 0.11.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec-derive | 0.11.6 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zmij | 1.0.23 | MIT | https://github.com/dtolnay/zmij |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | https://github.com/etemesi254/zune-image |
| zune-inflate | 0.2.54 | MIT OR Apache-2.0 OR Zlib | https://github.com/etemesi254/zune-image/tree/main/zune-inflate |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |

MPL-2.0 の部品は、ソースを改変せずに使っています。ソースは上の入手先と crates.io から入手できます。

## 著作権表示

### ECMA-376（Ecma International）

```text
The files below in this directory are taken unmodified from
ECMA-376 Office Open XML File Formats, 5th edition, Part 1 (December 2016),
ECMA-376-1_5th_edition_december_2016.zip:
https://ecma-international.org/publications-and-standards/standards/ecma-376/

- presetShapeDefinitions.xml, from OfficeOpenXML-DrawingMLGeometries.zip
- presetTextWarpDefinitions.xml, from OfficeOpenXML-DrawingMLGeometries.zip
- presetTableStyles.xml, from OfficeOpenXML-SpreadsheetMLStyles.zip
- presetCellStyles.xml, from OfficeOpenXML-SpreadsheetMLStyles.zip

They are used under the Ecma text copyright policy, permission (iv): "works by
making use of this specification in standard conformant products by
implementing (e.g. by copy and paste wholly or partly) the functionality
therein." The notice below applies to them.

COPYRIGHT NOTICE

© Ecma International

By obtaining and/or copying this work, you (the licensee) agree that you have read, understood, and will comply with the following terms and conditions.

This document may be copied, published and distributed to others, and certain derivative works of it may be prepared, copied, published, and distributed, in whole or in part, provided that the above copyright notice and this Copyright License and Disclaimer are included on all such copies and derivative works. The only derivative works that are permissible under this Copyright License and Disclaimer are:

(i) works which incorporate all or portion of this document for the purpose of providing commentary or explanation (such as an annotated version of the document),

(ii) works which incorporate all or portion of this document for the purpose of incorporating features that provide accessibility,

(iii) translations of this document into languages other than English and into different formats and

(iv) works by making use of this specification in standard conformant products by implementing (e.g. by copy and paste wholly or partly) the functionality therein.

However, the content of this document itself may not be modified in any way, including by removing the copyright notice or references to Ecma International, except as required to translate it into languages other than English or into a different format.

The official version of an Ecma International document is the English language version on the Ecma International website. In the event of discrepancies between a translated version and the official version, the official version shall govern.

The limited permissions granted above are perpetual and will not be revoked by Ecma International or its successors or assigns.

This document and the information contained herein is provided on an "AS IS" basis and ECMA INTERNATIONAL DISCLAIMS ALL WARRANTIES, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTY THAT THE USE OF THE INFORMATION HEREIN WILL NOT INFRINGE ANY OWNERSHIP RIGHTS OR ANY IMPLIED WARRANTIES OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
```

```text
OfficeOpenXML-WordprocessingMLArtBorders.zip in this directory is taken
unmodified (the zip file itself, as distributed) from ECMA-376 Office Open XML
File Formats, 5th edition, Part 1 (December 2016),
ECMA-376-1_5th_edition_december_2016.zip
(SHA-256 9d0bcad9cf06054785b03762fcfadbf6bab7e54a5f9d69434e34b7fd464d4129):
https://ecma-international.org/publications-and-standards/standards/ecma-376/

OfficeOpenXML-WordprocessingMLArtBorders.zip
SHA-256 f9894d9a26f7b59ba2b233fe87930c9f0a1ec17967fe66f3d2ddf83b79c631ff
It holds the pictures of the art page borders (ST_Border), 1,320 PNG files
named <border>_<position>.png. ibisOffice reads the pictures of the borders a
document uses from the zip when it draws them.

It is used under the Ecma text copyright policy, permission (iv): "works by
making use of this specification in standard conformant products by
implementing (e.g. by copy and paste wholly or partly) the functionality
therein." The notice below applies to it.

COPYRIGHT NOTICE

© Ecma International

By obtaining and/or copying this work, you (the licensee) agree that you have read, understood, and will comply with the following terms and conditions.

This document may be copied, published and distributed to others, and certain derivative works of it may be prepared, copied, published, and distributed, in whole or in part, provided that the above copyright notice and this Copyright License and Disclaimer are included on all such copies and derivative works. The only derivative works that are permissible under this Copyright License and Disclaimer are:

(i) works which incorporate all or portion of this document for the purpose of providing commentary or explanation (such as an annotated version of the document),

(ii) works which incorporate all or portion of this document for the purpose of incorporating features that provide accessibility,

(iii) translations of this document into languages other than English and into different formats and

(iv) works by making use of this specification in standard conformant products by implementing (e.g. by copy and paste wholly or partly) the functionality therein.

However, the content of this document itself may not be modified in any way, including by removing the copyright notice or references to Ecma International, except as required to translate it into languages other than English or into a different format.

The official version of an Ecma International document is the English language version on the Ecma International website. In the event of discrepancies between a translated version and the official version, the official version shall govern.

The limited permissions granted above are perpetual and will not be revoked by Ecma International or its successors or assigns.

This document and the information contained herein is provided on an "AS IS" basis and ECMA INTERNATIONAL DISCLAIMS ALL WARRANTIES, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTY THAT THE USE OF THE INFORMATION HEREIN WILL NOT INFRINGE ANY OWNERSHIP RIGHTS OR ANY IMPLIED WARRANTIES OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
```

### IBM Plex Sans

```text
Font copyrights:
- Copyright 2017 IBM Corp. with Reserved Font Name 'Plex'

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### Lucide

```text
ISC License

Copyright (c) 2026 Lucide Icons and Contributors

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

---

The following Lucide icons are derived from the Feather project:

airplay, alert-circle, alert-octagon, alert-triangle, aperture, arrow-down-circle, arrow-down-left, arrow-down-right, arrow-down, arrow-left-circle, arrow-left, arrow-right-circle, arrow-right, arrow-up-circle, arrow-up-left, arrow-up-right, arrow-up, at-sign, calendar, cast, check, chevron-down, chevron-left, chevron-right, chevron-up, chevrons-down, chevrons-left, chevrons-right, chevrons-up, circle, clipboard, clock, code, columns, command, compass, corner-down-left, corner-down-right, corner-left-down, corner-left-up, corner-right-down, corner-right-up, corner-up-left, corner-up-right, crosshair, database, divide-circle, divide-square, dollar-sign, download, external-link, feather, frown, hash, headphones, help-circle, info, italic, key, layout, life-buoy, link-2, link, loader, lock, log-in, log-out, maximize, meh, minimize, minimize-2, minus-circle, minus-square, minus, monitor, moon, more-horizontal, more-vertical, move, music, navigation-2, navigation, octagon, pause-circle, percent, plus-circle, plus-square, plus, power, radio, rss, search, server, share, shopping-bag, sidebar, smartphone, smile, square, table-2, tablet, target, terminal, trash-2, trash, triangle, tv, type, upload, x-circle, x-octagon, x-square, x, zoom-in, zoom-out

The MIT License (MIT) (for the icons listed above)

Copyright (c) 2013-present Cole Bemis

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Material Symbols（Google）

Copyright Google LLC. Apache License 2.0（本文は下の「ライセンス本文」の Apache-2.0）。 出典：Google 公式のリポジトリ https://github.com/google/material-design-icons のコミット `bd8cb85bd4bad964fe6918f79665bb40c3a8efef`（2026-09-25）。
ファイル名から寸法の接尾辞（`_20px`）を除き、塗りつぶし版は `_fill` と名付けた。図形は変えていない。

### Rust の部品

各部品のライセンス ファイルにある著作権表示です（ライセンス ファイルに表示がない部品は、上の一覧のライセンスと入手先に従います）。

- **addr2line 0.25.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016-2018 The gimli Developers
- **adler2 2.0.1**：Copyright (C) Jonas Schievink <jonasschievink@gmail.com> / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **aes 0.8.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Artyom Pavlov
- **aho-corasick 1.1.5**：Copyright (c) 2015 Andrew Gallant
- **annotate-snippets 0.12.16**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **anstyle 1.0.14**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **anyhow 1.0.104**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **arc-swap 1.9.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 arc-swap developers
- **arraydeque 0.5.1**：Copyright (c) 2018 Andy Lok <andylokandy@hotmail.com>
- **arrayref 0.3.9**：Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>
- **arrayvec 0.7.8**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Ulrik Sverdrup "bluss" 2015-2023
- **async-channel 2.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **async-compression 0.4.48**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 the rustasync developers
- **async-task 4.7.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **atomic 0.5.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 The Rust Project Developers
- **backtrace 0.3.76**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **base62 2.2.6**：Copyright (c) 2015 François Bernier
- **base64 0.22.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 Alice Maz
- **base64 0.23.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 Alice Maz, Marshall Pierce
- **bit-set 0.9.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2026 The Rust Project Developers
- **bit-vec 0.9.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2023 The Rust Project Developers
- **bit_field 0.10.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Philipp Oppermann
- **bitflags 1.3.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **bitflags 2.13.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **block-buffer 0.10.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2019 The RustCrypto Project Developers
- **block-padding 0.3.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2019 The RustCrypto Project Developers
- **bstr 1.13.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2019 Andrew Gallant
- **bumpalo 3.20.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 Nick Fitzgerald
- **bytemuck 1.25.2**：Copyright (c) 2019 Daniel "Lokathor" Gee.
- **bytemuck_derive 1.12.1**：Copyright (c) 2019 Daniel "Lokathor" Gee.
- **byteorder 1.5.0**：Copyright (c) 2015 Andrew Gallant
- **byteorder-lite 0.1.0**：Copyright (c) 2015 Andrew Gallant
- **bytes 1.12.1**：Copyright (c) 2018 Carl Lerche
- **bzip2 0.6.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014-2025 Alex Crichton and Contributors
- **cbc 0.1.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2022 RustCrypto Developers / Copyright (c) 2018 Artyom Pavlov
- **cfg-if 1.0.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **chrono 0.4.45**：Copyright (c) 2014, Kang Seonghoon. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **cipher 0.4.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016-2020 RustCrypto Developers
- **codespan-reporting 0.13.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **color_quant 1.1.0**：Copyright (c) 2016 PistonDevelopers
- **compression-codecs 0.4.43**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 the rustasync developers
- **compression-core 0.4.33**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 the rustasync developers
- **concurrent-queue 2.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **console_error_panic_hook 0.1.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Nick Fitzgerald
- **convert_case 0.10.0**：Copyright (c) 2025 rutrum
- **core_maths 0.1.1**：Copyright (c) 2024 Robert Bastian
- **cosmic-text 0.19.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2022 System76
- **crc32fast 1.5.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors
- **crossbeam-deque 0.8.8**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 The Crossbeam Project Developers
- **crossbeam-epoch 0.9.21**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 The Crossbeam Project Developers
- **crossbeam-queue 0.3.14**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 The Crossbeam Project Developers
- **crossbeam-utils 0.8.23**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 The Crossbeam Project Developers
- **crypto-common 0.1.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2021 RustCrypto Developers
- **ctor 1.0.13**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **data-url 0.3.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2013-2025 The rust-url developers
- **derive_more 2.1.1**：Copyright (c) 2016 Jelte Fennema
- **derive_more-impl 2.1.1**：Copyright (c) 2016 Jelte Fennema
- **digest 0.10.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 Artyom Pavlov
- **displaydoc 0.2.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **document-features 0.2.12**：Copyright (c) 2020 Olivier Goffart <ogoffart@sixtyfps.io>
- **dyn-clone 1.0.20**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **either 1.18.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015
- **encoding_rs 0.8.41**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright Mozilla Foundation / Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).
- **encoding_rs_io 0.1.8**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 Andrew Gallant
- **enum-iterator 2.3.0**：Copyright (c) 2018 Stephane Raux
- **enum-iterator-derive 1.5.0**：Copyright (c) 2018 Stephane Raux
- **enumn 0.1.14**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **equivalent 1.0.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016--2023
- **erased-serde 0.4.10**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **etagere 0.2.15**：Copyright 2020 Nicolas Silva / Copyright (c) 2020 Nicolas Silva / COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
- **euclid 0.22.14**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2012-2013 Mozilla Foundation
- **event-listener 5.4.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **event-listener-strategy 0.5.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **exr 1.74.2**：Copyright (c) Contributors to the OpenEXR Project. All rights reserved. / Copyright (c) Contributors to the exrs Project. All rights reserved.
- **fastrand 2.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **fax 0.2.7**：Copyright © 2021 The pdf-rs contributers.
- **fdeflate 0.3.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **filetime 0.2.29**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **fixedbitset 0.5.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015-2017
- **flate2 1.1.10**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014-2026 Alex Crichton
- **float-cmp 0.9.0**：Copyright (c) 2014-2020 Optimal Computing (NZ) Ltd
- **float_next_after 1.0.0**：Copyright (c) 2020 Scripta Qumranica Electronica
- **fluent-uri 0.1.4**：Copyright (c) 2021 Scallop Ye
- **flume 0.12.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **foldhash 0.2.0**：Copyright (c) 2024 Orson Peters
- **font-types 0.11.3**：Copyright 2019 Colin Rothfels / Copyright (c) 2019 Colin Rothfels
- **font-types 0.12.5**：Copyright 2019 Fontations Developers / Copyright (c) 2019 Fontations Developers
- **fontdb 0.23.0**：Copyright (c) 2020 Yevhenii Reizner
- **form_urlencoded 1.2.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2013-2016 The rust-url developers
- **futf 0.1.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 Keegan McAllister
- **futures 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-channel 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-concurrency 7.7.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2020 Yoshua Wuyts / Copyright (c) 2020 Yoshua Wuyts
- **futures-core 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-executor 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-io 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-lite 2.6.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-macro 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-sink 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-task 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **futures-util 0.3.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Alex Crichton / Copyright (c) 2017 The Tokio Authors
- **generic-array 0.14.7**：Copyright (c) 2015 Bartłomiej Kamiński
- **getrandom 0.3.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2025 The rust-random Project Developers / Copyright (c) 2014 The Rust Project Developers
- **getrandom 0.4.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018-2026 The rust-random Project Developers / Copyright (c) 2014 The Rust Project Developers
- **gif 0.14.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 nwin
- **gimli 0.32.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **glob 0.3.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **globset 0.4.20**：Copyright (c) 2015 Andrew Gallant
- **globwalk 0.8.1**：Copyright (c) 2017 Gilad Naaman
- **glow 0.17.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-base 0.6.6**：Copyright 2024 - 2026 Longbridge <https://longbridge.com> / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-component 0.6.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2024 - 2026 Longbridge <https://longbridge.com>
- **gpui-component-macros 0.6.6**：Copyright 2024 - 2026 Longbridge <https://longbridge.com> / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-kit-assets 0.6.6**：Copyright (c) 2026 Lucide Icons and Contributors / copyright notice and this permission notice appear in all copies. / Copyright (c) 2013-present Cole Bemis
- **gpui-pre 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-collections 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-derive-refineable 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-http-client 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-macros 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-perf 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-platform 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-refineable 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-reqwest 0.12.15**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2016 Sean McArthur / Copyright (c) 2016-2025 Sean McArthur
- **gpui-pre-scheduler 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-shared-string 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-sum-tree 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-util 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-util-macros 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-web 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-wgpu 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-zlog 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-ztracing 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **gpui-pre-ztracing-macro 0.3.6**：Copyright 2022 - 2025 Zed Industries, Inc. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **half 2.7.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **hash32 0.3.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Jorge Aparicio
- **hashbrown 0.16.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Amanieu d'Antras
- **hashbrown 0.17.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Amanieu d'Antras
- **heapless 0.9.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 Jorge Aparicio
- **heck 0.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **hmac 0.12.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 Artyom Pavlov
- **html5ever 0.27.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The html5ever Project Developers
- **http 1.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2017 http-rs authors / Copyright (c) 2017 http-rs authors
- **http-body 1.1.0**：Copyright (c) 2019-2026 Sean McArthur & Hyper Contributors
- **icu_collections 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_locale_core 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_normalizer 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_normalizer_data 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_properties 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_properties_data 2.3.0**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **icu_provider 2.3.1**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **idna 1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2013-2025 The rust-url developers
- **idna_adapter 1.2.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) The rust-url developers
- **ignore 0.4.33**：Copyright (c) 2015 Andrew Gallant
- **image 0.25.10**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **image-webp 0.2.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **imagesize 0.14.0**：Copyright (c) 2017 Maiddog
- **indexmap 2.14.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016--2017
- **inout 0.1.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2022 The RustCrypto Project Developers / Copyright (c) 2022 Artyom Pavlov
- **instant 0.1.13**：Copyright (c) 2019, Sébastien Crozet
- **inventory 0.3.24**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **itertools 0.11.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015
- **itertools 0.13.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015
- **itertools 0.14.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015
- **itoa 1.0.18**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **js-sys 0.3.105**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **kurbo 0.13.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Raph Levien
- **lazy_static 1.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2010 The Rust Project Developers
- **lebe 0.5.3**：Copyright (c) 2022 Contributors to the lebe Project. All rights reserved.
- **libbz2-rs-sys 0.2.5**：Copyright (C) 1996-2021 Julian R Seward. / Copyright (C) 2019-2020 Federico Mena Quintero / Copyright (C) 2021 Micah Snyder
- **libc 0.2.189**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) The Rust Project Developers
- **libm 0.2.16**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Jorge Aparicio / copyright: / Copyright © 2005-2020 Rich Felker, et al. / Copyright © 1993,2004 Sun Microsystems or / Copyright © 2003-2011 David Schultz or / Copyright © 2003-2009 Steven G. Kargl or / Copyright © 2003-2009 Bruce D. Evans or / Copyright © 2008 Stephen L. Moshier or / Copyright © 2017-2018 Arm Limited / Copyright notices are retained in src/* files where relevant.
- **linebender_resource_handle 0.1.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **link-section 0.19.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **linktime-proc-macro 0.2.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **litemap 0.8.3**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **litrs 1.0.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Project Developers
- **lock_api 0.4.14**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 The Rust Project Developers
- **log 0.4.34**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **lsp-types 0.97.0**：Copyright (c) 2016 Markus Westerlind
- **markdown 1.0.0**：Copyright (c) 2022 Titus Wormer <tituswormer@gmail.com>
- **markup5ever 0.12.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The html5ever Project Developers
- **markup5ever_rcdom 0.3.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The html5ever Project Developers
- **md-5 0.10.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2006-2009 Graydon Hoare / Copyright (c) 2009-2013 Mozilla Foundation / Copyright (c) 2016 Artyom Pavlov
- **memchr 2.8.3**：Copyright (c) 2015 Andrew Gallant
- **memmap2 0.9.11**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright [2015] [Dan Burkert] / Copyright (c) 2020 Yevhenii Reizner / Copyright (c) 2015 Dan Burkert
- **mime 0.3.17**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Sean McArthur
- **mime_guess 2.0.5**：Copyright (c) 2015 Austin Bonander
- **miniz_oxide 0.8.9**：Copyright 2013-2014 RAD Game Tools and Valve Software / Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC / Copyright (c) 2017 Frommi / Copyright (c) 2017-2024 oyvindln / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Frommi
- **miniz_oxide 0.9.1**：Copyright 2013-2014 RAD Game Tools and Valve Software / Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC / Copyright (c) 2017 Frommi / Copyright (c) 2017-2024 oyvindln / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Frommi
- **moxcms 0.8.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2024 Radzivon Bartoshyk / Copyright (c) Radzivon Bartoshyk. All rights reserved.
- **multiversion_no_op 1.0.0**：Copyright Mozilla Foundation / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **naga 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **new_debug_unreachable 1.0.6**：Copyright (c) 2015 Jonathan Reem
- **nohash-hasher 0.2.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2018 Parity Technologies (UK) Ltd.
- **normpath 1.5.2**：Copyright (c) 2020 dylni (https://github.com/dylni) / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Nikolai Vazquez
- **notify-types 1.0.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2023 Notify Contributors / Copyright (c) 2023 Notify Contributors
- **nu-ansi-term 0.50.3**：Copyright (c) 2014 Benjamin Sago / Copyright (c) 2021-2022 The Nushell Project Developers
- **num-complex 0.4.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **num-traits 0.2.19**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **num_cpus 1.17.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015-2025 Sean McArthur
- **object 0.37.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Gimli Developers
- **once_cell 1.21.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **parking 2.2.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2014-2020 The Rust Project Developers
- **parking_lot 0.12.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 The Rust Project Developers
- **parking_lot_core 0.9.12**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 The Rust Project Developers
- **paste 1.0.15**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **percent-encoding 2.3.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2013-2025 The rust-url developers
- **phf 0.11.3**：Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi / COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
- **phf_shared 0.11.3**：Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi / COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
- **pico-args 0.5.0**：Copyright (c) 2019 Yevhenii Reizner
- **pin-project 1.1.13**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **pin-project-internal 1.1.13**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **pin-project-lite 0.2.17**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **png 0.17.16**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 nwin
- **png 0.18.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 nwin
- **pollster 0.2.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020-2021 Joshua Barretto
- **pollster 0.4.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020-2021 Joshua Barretto
- **polycool 0.4.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Raph Levien
- **postage 0.5.0**：Copyright (c) 2022 Austin Jones
- **potential_utf 0.1.6**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **ppv-lite86 0.2.21**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2019 The CryptoCorrosion Contributors / Copyright (c) 2019 The CryptoCorrosion Contributors
- **precomputed-hash 0.1.1**：Copyright (c) 2017 Emilio Cobos Álvarez
- **proc-macro-crate 3.5.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **proc-macro2 1.0.107**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **pulp 0.22.3**：Copyright (c) 2021 sarah
- **pxfm 0.1.30**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2024 Radzivon Bartoshyk / Copyright (c) Radzivon Bartoshyk. All rights reserved.
- **qoi 0.4.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2022 Ivan Smirnov
- **quick-error 2.0.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The quick-error Developers
- **quote 1.0.47**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **rand 0.9.5**：Copyrights in the Rand project are retained by their contributors. No / copyright assignment is required to contribute to the Rand project. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2018 Developers of the Rand project / Copyright (c) 2014 The Rust Project Developers
- **rand_chacha 0.9.0**：Copyrights in the Rand project are retained by their contributors. No / copyright assignment is required to contribute to the Rand project. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2018 Developers of the Rand project / Copyright (c) 2014 The Rust Project Developers
- **rand_core 0.9.5**：Copyrights in the Rand project are retained by their contributors. No / copyright assignment is required to contribute to the Rand project. / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2018 Developers of the Rand project / Copyright (c) 2014 The Rust Project Developers
- **rangemap 1.8.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2019-2022 Jeff Parsons, and [contributors](https://github.com/jeffparsons/rangemap/contributors) / Copyright 2019 Jeffrey Parsons
- **raw-window-handle 0.6.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 Osspial / Copyright (c) 2020 Osspial
- **rayon 1.12.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2010 The Rust Project Developers
- **rayon-core 1.13.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2010 The Rust Project Developers
- **read-fonts 0.37.0**：Copyright 2019 Colin Rothfels / Copyright (c) 2019 Colin Rothfels
- **read-fonts 0.41.0**：Copyright 2019 Fontations Developers / Copyright (c) 2019 Fontations Developers
- **reborrow 0.5.5**：Copyright (c) 2022 sarah
- **ref-cast 1.0.27**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **ref-cast-impl 1.0.27**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **regex 1.13.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **regex-automata 0.4.18**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **regex-syntax 0.8.11**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers
- **resvg 0.46.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2017 the Resvg Authors
- **rgb 0.8.53**：Copyright (c) 2019 Kornel
- **ropey 2.0.0-beta.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2023 Nathan Vegdahl
- **roxmltree 0.21.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Yevhenii Reizner
- **rust-i18n 4.2.2**：Copyright (c) 2021 Longbridge
- **rustc-demangle 0.1.28**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **rustc-hash 1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **rustc-hash 2.1.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **rustversion 1.0.23**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **rustybuzz 0.20.1**：Copyright (c) HarfBuzz developers / Copyright (c) 2020 Yevhenii Reizner
- **ryu 1.0.23**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **same-file 1.0.6**：Copyright (c) 2017 Andrew Gallant
- **schemars 1.2.2**：Copyright (c) 2019 Graham Esau
- **schemars_derive 1.2.2**：Copyright (c) 2019 Graham Esau
- **scopeguard 1.2.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers
- **self_cell 1.3.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (C) 1989, 1991 Free Software Foundation, Inc., / copyright notice and disclaimer of warranty; keep intact all the
- **serde 1.0.229**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde-saphyr 1.3.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_core 1.0.229**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_derive 1.0.229**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_derive_internals 0.30.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_fmt 1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 Ashley Mannix
- **serde_json 1.0.151**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_repr 0.1.21**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **serde_spanned 0.6.9**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **serde_urlencoded 0.7.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 Anthony Ramine
- **sha1 0.10.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2006-2009 Graydon Hoare / Copyright (c) 2009-2013 Mozilla Foundation / Copyright (c) 2016 Artyom Pavlov
- **sha1_smol 1.0.1**：Copyright (c) 2018, the respective contributors, as shown by the AUTHORS file. / Copyright (c) 2006-2009 Graydon Hoare / Copyright (c) 2009-2013 Mozilla Foundation
- **sha2 0.10.9**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2006-2009 Graydon Hoare / Copyright (c) 2009-2013 Mozilla Foundation / Copyright (c) 2016 Artyom Pavlov
- **sharded-slab 0.1.7**：Copyright (c) 2019 Eliza Weisman
- **simd-adler32 0.3.10**：Copyright (c) [2021] [Marvin Countryman]
- **simdutf8 0.1.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **simplecss 0.2.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Reizner Evgeniy
- **siphasher 1.0.3**：Copyright 2012-2016 The Rust Project Developers. / Copyright 2016-2026 Frank Denis.
- **skrifa 0.40.0**：Copyright 2019 Colin Rothfels / Copyright (c) 2019 Colin Rothfels
- **skrifa 0.44.0**：Copyright 2019 Fontations Developers / Copyright (c) 2019 Fontations Developers
- **slab 0.4.12**：Copyright (c) 2019 Carl Lerche
- **slotmap 1.1.1**：Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com>
- **smallvec 1.16.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 The Servo Project Developers
- **smol_str 0.3.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **spin 0.10.1**：Copyright (c) 2014 Mathijs van de Nes
- **spin 0.9.9**：Copyright (c) 2014 Mathijs van de Nes
- **stable_deref_trait 1.2.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 Robert Grosse
- **static_assertions 1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2017 Nikolai Vazquez
- **str_indices 0.4.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **strict-num 0.1.1**：Copyright (c) 2022 Yevhenii Reizner
- **string_cache 0.8.9**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2012-2013 Mozilla Foundation
- **strum 0.28.0**：Copyright (c) 2019 Peter Glotfelty
- **strum_macros 0.28.0**：Copyright (c) 2019 Peter Glotfelty
- **subtle 2.6.1**：Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved. / Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.
- **svgtypes 0.16.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Yevhenii Reizner
- **swash 0.2.10**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Chad Brokaw
- **syn 2.0.119**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **syn 3.0.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **sync_wrapper 1.0.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **synstructure 0.14.0**：Copyright 2016 Nika Layzell
- **sys-locale 0.3.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2021 1Password
- **tendril 0.4.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 Keegan McAllister
- **thiserror 1.0.69**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **thiserror 2.0.21**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **thiserror-impl 1.0.69**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **thiserror-impl 2.0.21**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **thread_local 1.1.10**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2016 The Rust Project Developers
- **tiff 0.11.3**：Copyright (c) 2018 PistonDevelopers
- **tiny-skia 0.11.4**：Copyright (c) 2011 Google Inc. All rights reserved. / Copyright (c) 2020 Yevhenii Reizner All rights reserved.
- **tiny-skia-path 0.11.4**：Copyright (c) 2011 Google Inc. All rights reserved. / Copyright (c) 2020 Yevhenii Reizner All rights reserved.
- **tinystr 0.8.4**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **tinyvec 1.13.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 Daniel "Lokathor" Gee.
- **toml 0.8.23**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_datetime 0.6.11**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_datetime 1.1.1+spec-1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_edit 0.22.27**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_edit 0.25.15+spec-1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_parser 1.1.3+spec-1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **toml_write 0.1.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) Individual contributors
- **tower-service 0.3.3**：Copyright (c) 2019 Tower Contributors
- **tracing 0.1.44**：Copyright (c) 2019 Tokio Contributors
- **tracing-attributes 0.1.31**：Copyright (c) 2019 Tokio Contributors
- **tracing-core 0.1.36**：Copyright (c) 2019 Tokio Contributors
- **tracing-log 0.2.0**：Copyright (c) 2019 Tokio Contributors
- **tracing-subscriber 0.3.23**：Copyright (c) 2019 Tokio Contributors
- **triomphe 0.1.16**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2019 Manish Goregaokar
- **ttf-parser 0.25.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Yevhenii Reizner
- **typeid 1.0.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **typenum 1.20.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2014 Paho Lurie-Gregg / Copyright (c) 2014 Paho Lurie-Gregg
- **unicase 2.9.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014-2026 Sean McArthur
- **unicode-bidi 0.3.18**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **unicode-bidi-mirroring 0.4.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Yevhenii Reizner
- **unicode-ccc 0.4.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Yevhenii Reizner
- **unicode-id 0.3.7**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **unicode-ident 1.0.26**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / COPYRIGHT AND PERMISSION NOTICE / Copyright © 1991-2023 Unicode, Inc.
- **unicode-linebreak 0.1.5**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **unicode-properties 0.1.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **unicode-script 0.5.8**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2021 The Unicode-rs Developers / Copyright (c) 2019 Manish Goregaokar
- **unicode-segmentation 1.13.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **unicode-vo 0.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2018 Reizner Evgeniy
- **unicode-width 0.2.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **unicode-xid 0.2.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2015 The Rust Project Developers
- **url 2.5.8**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2013-2025 The rust-url developers
- **usvg 0.46.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2017 the Resvg Authors
- **utf-8 0.7.6**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **utf8_iter 1.0.4**：Copyright Mozilla Foundation / copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **uuid 1.26.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The Rust Project Developers / Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon
- **value-bag 1.14.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 sval-rs
- **value-bag-serde1 1.14.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 sval-rs
- **waker-fn 1.2.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **walkdir 2.5.0**：Copyright (c) 2015 Andrew Gallant
- **wasm-bindgen 0.2.128**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **wasm-bindgen-futures 0.4.78**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **wasm-bindgen-macro 0.2.128**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **wasm-bindgen-macro-support 0.2.128**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **wasm-bindgen-shared 0.2.128**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **wasm-streams 0.4.2**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **wasm_thread 0.3.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works
- **web-sys 0.3.105**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 Alex Crichton
- **web-time 1.1.0**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2023 dAxpeDDa / Copyright (c) 2023 dAxpeDDa
- **weezl 0.1.12**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) HeroicKatora 2020
- **wgpu 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **wgpu-core 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **wgpu-core-deps-wasm 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **wgpu-hal 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **wgpu-naga-bridge 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **wgpu-types 29.0.4**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2025 The gfx-rs developers
- **writeable 0.6.4**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **xml5ever 0.18.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2014 The html5ever Project Developers
- **xmlwriter 0.1.0**：Copyright (c) 2019 Reizner Evgeniy
- **yazi 0.2.1**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Chad Brokaw
- **yoke 0.8.3**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **yoke-derive 0.8.3**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zeno 0.3.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) 2020 Chad Brokaw
- **zerocopy 0.8.58**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2023 The Fuchsia Authors / Copyright 2019 The Fuchsia Authors. / copyright notice, this list of conditions and the following disclaimer
- **zerocopy-derive 0.8.58**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright 2023 The Fuchsia Authors / Copyright 2019 The Fuchsia Authors. / copyright notice, this list of conditions and the following disclaimer
- **zerofrom 0.1.8**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zerofrom-derive 0.1.8**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zerotrie 0.2.5**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zerovec 0.11.8**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zerovec-derive 0.11.6**：COPYRIGHT AND PERMISSION NOTICE / Copyright © 2020-2024 Unicode, Inc.
- **zune-core 0.5.3**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) zune-image developers / (C) zune-image developers
- **zune-jpeg 0.5.15**：copyright notice that is included in or attached to the work / copyright license to reproduce, prepare Derivative Works of, / (c) You must retain, in the Source form of any Derivative Works / Copyright (c) zune-image developers / (C) zune-image developers

## 単一のライセンス（MIT・Apache-2.0 以外）の部品のライセンス本文

### arrayref 0.3.9（BSD-2-Clause）

```text
Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### enum-iterator 2.3.0（0BSD）

```text
BSD Zero Clause License

Copyright (c) 2018 Stephane Raux

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

### enum-iterator-derive 1.5.0（0BSD）

```text
BSD Zero Clause License

Copyright (c) 2018 Stephane Raux

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

### exr 1.74.2（BSD-3-Clause）

```text
# Licenses

This Rust implementation of the OpenEXR image format
and the exrs crate are not affiliated with the 
[OpenEXR project](https://www.openexr.com/) or the 
[ACADEMY SOFTWARE FOUNDATION](https://www.aswf.io/).


## The OpenEXR Image Format: [BSD-3-Clause](https://github.com/AcademySoftwareFoundation/openexr/blob/master/LICENSE.md)
Copyright (c) Contributors to the OpenEXR Project. All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## The `exrs` Project, and independent Rust implementation of the official OpenEXR Image Format: Also [BSD-3-Clause](https://github.com/AcademySoftwareFoundation/openexr/blob/master/LICENSE.md)
Copyright (c) Contributors to the exrs Project. All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### foldhash 0.2.0（Zlib）

```text
Copyright (c) 2024 Orson Peters

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

### icu_collections 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_locale_core 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_normalizer 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_normalizer_data 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_properties 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_properties_data 2.3.0（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### icu_provider 2.3.1（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### instant 0.1.13（BSD-3-Clause）

```text
Copyright (c) 2019, Sébastien Crozet
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the author nor the names of its contributors may be used
   to endorse or promote products derived from this software without specific
   prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### lebe 0.5.3（BSD-3-Clause）

```text
Copyright (c) 2022 Contributors to the lebe Project. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
this list of conditions and the following disclaimer in the documentation
and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
may be used to endorse or promote products derived from this software without
specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### libbz2-rs-sys 0.2.5（bzip2-1.0.6）

```text
--------------------------------------------------------------------------

The original program, "bzip2", the associated library "libbzip2", and all
documentation, are

Copyright (C) 1996-2021 Julian R Seward.
Copyright (C) 2019-2020 Federico Mena Quintero
Copyright (C) 2021 Micah Snyder

This Rust translation, "libbzip2-rs" is a derived work based on "bzip2" and
"libbzip2", and is Copyright (C) 2024-2025 Trifecta Tech Foundation and contributors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. The origin of this software must not be misrepresented; you must
   not claim that you wrote the original software.  If you use this
   software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.

3. Altered source versions must be plainly marked as such, and must
   not be misrepresented as being the original software.

4. The name of the author may not be used to endorse or promote
   products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS
OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Julian Seward, jseward@acm.org
bzip2/libbzip2 version 1.1.0 of 6 September 2010

--------------------------------------------------------------------------
```

### litemap 0.8.3（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### notify 7.0.0（CC0-1.0）

```text
Creative Commons CC0 1.0 Universal

<<beginOptional;name=ccOptionalIntro>> CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER.  <<endOptional>>

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer exclusive Copyright and Related Rights (defined below) upon the creator and subsequent owner(s) (each and all, an "owner") of an original work of authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for the purpose of contributing to a commons of creative, cultural and scientific works ("Commons") that the public can reliably and without fear of later claims of infringement build upon, modify, incorporate in other works, reuse and redistribute as freely as possible in any form whatsoever and for any purposes, including without limitation commercial purposes. These owners may contribute to the Commons to promote the ideal of a free culture and the further production of creative, cultural and scientific works, or to gain reputation or greater distribution for their Work in part through the use and efforts of others.

For these and/or other purposes and motivations, and without any expectation of additional consideration or compensation, the person associating CC0 with a Work (the "Affirmer"), to the extent that he or she is an owner of Copyright and Related Rights in the Work, voluntarily elects to apply CC0 to the Work and publicly distribute the Work under its terms, with knowledge of his or her Copyright and Related Rights in the Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be protected by copyright and related or neighboring rights ("Copyright and Related Rights"). Copyright and Related Rights include, but are not limited to, the following:

     i. the right to reproduce, adapt, distribute, perform, display, communicate, and translate a Work;

     ii. moral rights retained by the original author(s) and/or performer(s);

     iii. publicity and privacy rights pertaining to a person's image or likeness depicted in a Work;

     iv. rights protecting against unfair competition in regards to a Work, subject to the limitations in paragraph 4(a), below;

     v. rights protecting the extraction, dissemination, use and reuse of data in a Work;

     vi. database rights (such as those arising under Directive 96/9/EC of the European Parliament and of the Council of 11 March 1996 on the legal protection of databases, and under any national implementation thereof, including any amended or successor version of such directive); and

     vii. other similar, equivalent or corresponding rights throughout the world based on applicable law or treaty, and any national implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention of, applicable law, Affirmer hereby overtly, fully, permanently, irrevocably and unconditionally waives, abandons, and surrenders all of Affirmer's Copyright and Related Rights and associated claims and causes of action, whether now known or unknown (including existing as well as future claims and causes of action), in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each member of the public at large and to the detriment of Affirmer's heirs and successors, fully intending that such Waiver shall not be subject to revocation, rescission, cancellation, termination, or any other legal or equitable action to disrupt the quiet enjoyment of the Work by the public as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason be judged legally invalid or ineffective under applicable law, then the Waiver shall be preserved to the maximum extent permitted taking into account Affirmer's express Statement of Purpose. In addition, to the extent the Waiver is so judged Affirmer hereby grants to each affected person a royalty-free, non transferable, non sublicensable, non exclusive, irrevocable and unconditional license to exercise Affirmer's Copyright and Related Rights in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "License"). The License shall be deemed effective as of the date CC0 was applied by Affirmer to the Work. Should any part of the License for any reason be judged legally invalid or ineffective under applicable law, such partial invalidity or ineffectiveness shall not invalidate the remainder of the License, and in such case Affirmer hereby affirms that he or she will not (i) exercise any of his or her remaining Copyright and Related Rights in the Work or (ii) assert any associated claims and causes of action with respect to the Work, in either case contrary to Affirmer's express Statement of Purpose.

4. Limitations and Disclaimers.

     a. No trademark or patent rights held by Affirmer are waived, abandoned, surrendered, licensed or otherwise affected by this document.

     b. Affirmer offers the Work as-is and makes no representations or warranties of any kind concerning the Work, express, implied, statutory or otherwise, including without limitation warranties of title, merchantability, fitness for a particular purpose, non infringement, or the absence of latent or other defects, accuracy, or the present or absence of errors, whether or not discoverable, all to the greatest extent permissible under applicable law.

     c. Affirmer disclaims responsibility for clearing rights of other persons that may apply to the Work or any use thereof, including without limitation any person's Copyright and Related Rights in the Work. Further, Affirmer disclaims responsibility for obtaining any necessary consents, permissions or other rights required for any use of the Work.

     d. Affirmer understands and acknowledges that Creative Commons is not a party to this document and has no duty or obligation with respect to this CC0 or use of the Work.
```

### potential_utf 0.1.6（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### sha1_smol 1.0.1（BSD-3-Clause）

```text
BSD 3-Clause License

Copyright (c) 2018, the respective contributors, as shown by the AUTHORS file.
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright notice, this
  list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.

* Neither the name of the copyright holder nor the names of its
  contributors may be used to endorse or promote products derived from
  this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

-----

src/simd.rs is licensed under the MIT license:

Copyright (c) 2006-2009 Graydon Hoare
Copyright (c) 2009-2013 Mozilla Foundation

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### slotmap 1.1.1（Zlib）

```text
Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com>

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

 1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

 2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

 3. This notice may not be removed or altered from any source distribution.
```

### subtle 2.6.1（BSD-3-Clause）

```text
Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.
Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### tiny-skia 0.11.4（BSD-3-Clause）

```text
Copyright (c) 2011 Google Inc. All rights reserved.
Copyright (c) 2020 Yevhenii Reizner All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived
    from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### tiny-skia-path 0.11.4（BSD-3-Clause）

```text
Copyright (c) 2011 Google Inc. All rights reserved.
Copyright (c) 2020 Yevhenii Reizner All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived
    from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### tinystr 0.8.4（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### writeable 0.6.4（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### yoke 0.8.3（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### yoke-derive 0.8.3（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### zerofrom 0.1.8（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### zerofrom-derive 0.1.8（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### zerotrie 0.2.5（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### zerovec 0.11.8（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### zerovec-derive 0.11.6（Unicode-3.0）

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```


## ライセンス本文

複数のライセンスから選べる部品は、MIT または Apache-2.0 を選んでいます。

### Apache-2.0

```text
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

### BSD-2-Clause

```text
Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu>
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the
   distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### BSD-3-Clause

```text
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### BSL-1.0

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### CC0-1.0

```text
Creative Commons CC0 1.0 Universal

<<beginOptional;name=ccOptionalIntro>> CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER.  <<endOptional>>

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer exclusive Copyright and Related Rights (defined below) upon the creator and subsequent owner(s) (each and all, an "owner") of an original work of authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for the purpose of contributing to a commons of creative, cultural and scientific works ("Commons") that the public can reliably and without fear of later claims of infringement build upon, modify, incorporate in other works, reuse and redistribute as freely as possible in any form whatsoever and for any purposes, including without limitation commercial purposes. These owners may contribute to the Commons to promote the ideal of a free culture and the further production of creative, cultural and scientific works, or to gain reputation or greater distribution for their Work in part through the use and efforts of others.

For these and/or other purposes and motivations, and without any expectation of additional consideration or compensation, the person associating CC0 with a Work (the "Affirmer"), to the extent that he or she is an owner of Copyright and Related Rights in the Work, voluntarily elects to apply CC0 to the Work and publicly distribute the Work under its terms, with knowledge of his or her Copyright and Related Rights in the Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be protected by copyright and related or neighboring rights ("Copyright and Related Rights"). Copyright and Related Rights include, but are not limited to, the following:

     i. the right to reproduce, adapt, distribute, perform, display, communicate, and translate a Work;

     ii. moral rights retained by the original author(s) and/or performer(s);

     iii. publicity and privacy rights pertaining to a person's image or likeness depicted in a Work;

     iv. rights protecting against unfair competition in regards to a Work, subject to the limitations in paragraph 4(a), below;

     v. rights protecting the extraction, dissemination, use and reuse of data in a Work;

     vi. database rights (such as those arising under Directive 96/9/EC of the European Parliament and of the Council of 11 March 1996 on the legal protection of databases, and under any national implementation thereof, including any amended or successor version of such directive); and

     vii. other similar, equivalent or corresponding rights throughout the world based on applicable law or treaty, and any national implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention of, applicable law, Affirmer hereby overtly, fully, permanently, irrevocably and unconditionally waives, abandons, and surrenders all of Affirmer's Copyright and Related Rights and associated claims and causes of action, whether now known or unknown (including existing as well as future claims and causes of action), in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each member of the public at large and to the detriment of Affirmer's heirs and successors, fully intending that such Waiver shall not be subject to revocation, rescission, cancellation, termination, or any other legal or equitable action to disrupt the quiet enjoyment of the Work by the public as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason be judged legally invalid or ineffective under applicable law, then the Waiver shall be preserved to the maximum extent permitted taking into account Affirmer's express Statement of Purpose. In addition, to the extent the Waiver is so judged Affirmer hereby grants to each affected person a royalty-free, non transferable, non sublicensable, non exclusive, irrevocable and unconditional license to exercise Affirmer's Copyright and Related Rights in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "License"). The License shall be deemed effective as of the date CC0 was applied by Affirmer to the Work. Should any part of the License for any reason be judged legally invalid or ineffective under applicable law, such partial invalidity or ineffectiveness shall not invalidate the remainder of the License, and in such case Affirmer hereby affirms that he or she will not (i) exercise any of his or her remaining Copyright and Related Rights in the Work or (ii) assert any associated claims and causes of action with respect to the Work, in either case contrary to Affirmer's express Statement of Purpose.

4. Limitations and Disclaimers.

     a. No trademark or patent rights held by Affirmer are waived, abandoned, surrendered, licensed or otherwise affected by this document.

     b. Affirmer offers the Work as-is and makes no representations or warranties of any kind concerning the Work, express, implied, statutory or otherwise, including without limitation warranties of title, merchantability, fitness for a particular purpose, non infringement, or the absence of latent or other defects, accuracy, or the present or absence of errors, whether or not discoverable, all to the greatest extent permissible under applicable law.

     c. Affirmer disclaims responsibility for clearing rights of other persons that may apply to the Work or any use thereof, including without limitation any person's Copyright and Related Rights in the Work. Further, Affirmer disclaims responsibility for obtaining any necessary consents, permissions or other rights required for any use of the Work.

     d. Affirmer understands and acknowledges that Creative Commons is not a party to this document and has no duty or obligation with respect to this CC0 or use of the Work.
```

### MIT

```text
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### Unicode-3.0

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### bzip2-1.0.6

```text
--------------------------------------------------------------------------

The original program, "bzip2", the associated library "libbzip2", and all
documentation, are

Copyright (C) 1996-2021 Julian R Seward.
Copyright (C) 2019-2020 Federico Mena Quintero
Copyright (C) 2021 Micah Snyder

This Rust translation, "libbzip2-rs" is a derived work based on "bzip2" and
"libbzip2", and is Copyright (C) 2024-2025 Trifecta Tech Foundation and contributors

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. The origin of this software must not be misrepresented; you must
   not claim that you wrote the original software.  If you use this
   software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.

3. Altered source versions must be plainly marked as such, and must
   not be misrepresented as being the original software.

4. The name of the author may not be used to endorse or promote
   products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS
OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Julian Seward, jseward@acm.org
bzip2/libbzip2 version 1.1.0 of 6 September 2010

--------------------------------------------------------------------------
```

