// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `#[editor_ext]` on code shaped like the editor's: the shell with the type, two formats with their methods and their
//! lists (`ext`), a caller outside. What is checked by compiling is as much as what is checked by running.

mod shell {
    pub struct Context<'a, T> {
        pub notified: &'a mut u32,
        pub entity: std::marker::PhantomData<T>,
    }

    impl<T: 'static> Context<'_, T> {
        pub fn notify(&mut self) {
            *self.notified += 1;
        }
        /// As GPUI's `cx.listener`: the closure is given the entity.
        pub fn listener(&mut self, f: impl Fn(&mut T, &mut Self) -> u32 + 'static) -> Box<dyn Fn(&mut T, &mut Self) -> u32> {
            Box::new(f)
        }
    }

    pub struct SlideEditor {
        pub words: Vec<String>,
        pub count: u32,
    }

    impl SlideEditor {
        pub fn new() -> Self {
            Self { words: vec!["one".into(), "two".into(), "three".into()], count: 0 }
        }
        /// A method of the shell, called from the blocks of the formats.
        pub fn touch(&mut self, cx: &mut Context<Self>) {
            self.count += 1;
            cx.notify();
        }
    }
}

mod docx {
    pub mod ext {
        pub(crate) use super::keys::KeysCrate as _;
        pub use super::keys::Keys as _;
        pub(crate) use super::panel::PanelCrate as _;
        pub(super) use super::panel::PanelSuper as _;
        #[cfg(not(any()))]
        pub(crate) use super::panel::Panel2Crate as _;
    }

    pub mod keys {
        use crate::docx::ext::*;
        use crate::shell::{Context, SlideEditor};

        /// What the format gives the shell: plain function pointers to its methods.
        pub struct Hooks {
            pub key_down: fn(&mut SlideEditor, &str, &mut Context<SlideEditor>) -> bool,
            pub word_count: fn(&SlideEditor) -> usize,
        }

        pub fn hooks() -> Hooks {
            Hooks { key_down: SlideEditor::key_down, word_count: SlideEditor::word_count }
        }

        #[ibis_office_macros::editor_ext(Keys)]
        impl SlideEditor {
            /// A key: the letter is counted, the panel of another file is opened, the shell is told.
            pub(crate) fn key_down(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
                if !Self::is_letter(key) {
                    return false;
                }
                self.open_panel(cx);
                self.touch(cx);
                self.remember(key.to_string())
            }

            /// No `self`: called as `Self::is_letter`.
            fn is_letter(key: &str) -> bool {
                key.len() == 1
            }

            /// A pattern in the arguments, a receiver taken apart in the body.
            fn remember(&mut self, mut word: String) -> bool {
                word.make_ascii_uppercase();
                let Self { words, .. } = self;
                words.push(word);
                true
            }

            pub fn word_count(&self) -> usize {
                self.words.len()
            }

            /// A closure that is given the editor calls a method of the block.
            pub(crate) fn listener_total(&mut self, cx: &mut Context<Self>) -> u32 {
                let listener = cx.listener(|this: &mut Self, cx| {
                    this.touch(cx);
                    this.count + this.longest(|w| w.len() > 3).map_or(0, |w| w.len() as u32)
                });
                listener(self, cx)
            }

            /// `impl Trait` in the arguments and a lifetime in what is returned.
            #[inline]
            #[must_use]
            pub(crate) fn longest<'a>(&'a self, keep: impl Fn(&str) -> bool) -> Option<&'a str> {
                self.words.iter().map(String::as_str).filter(|w| keep(w)).max_by_key(|w| w.len())
            }

            /// `impl Trait` returned, with a type parameter and a `where`.
            pub(crate) fn lengths<T>(&self, scale: T) -> impl Iterator<Item = T> + '_
            where
                T: Copy + std::ops::Mul<Output = T> + From<u8> + 'static,
            {
                self.words.iter().map(move |w| scale * T::from(w.len() as u8))
            }

            /// What is returned borrows nothing (`use<>`): the editor can be changed while it is held.
            pub(crate) fn counter(&self) -> impl Fn() -> u32 + use<> {
                let count = self.count;
                move || count
            }

            /// The receiver by value.
            pub(crate) fn into_words(mut self) -> Vec<String> {
                self.words.sort();
                self.words
            }

            /// Two methods of one name, one for each side of a `cfg`.
            #[cfg(any())]
            pub(crate) fn platform(&self) -> &'static str {
                "never built"
            }
            #[cfg(not(any()))]
            pub(crate) fn platform(&self) -> &'static str {
                "built"
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            /// A child module that takes everything of its parent sees the private trait and the list the parent imported.
            #[test]
            fn the_child_module_calls_private_and_imported_methods() {
                let mut notified = 0;
                let mut cx = Context { notified: &mut notified, entity: std::marker::PhantomData };
                let mut editor = SlideEditor::new();
                assert!(editor.remember("x".into()));
                assert!(SlideEditor::is_letter("x"));
                editor.open_panel(&mut cx);
                assert_eq!(editor.words.len(), 5);
            }
        }
    }

    pub mod panel {
        use crate::shell::{Context, SlideEditor};

        #[ibis_office_macros::editor_ext(Panel)]
        impl SlideEditor {
            pub(crate) fn open_panel(&mut self, cx: &mut Context<Self>) {
                self.words.push(self.panel_title().to_string());
                cx.notify();
            }
            /// For the files of the format only.
            pub(super) fn panel_title(&self) -> &'static str {
                "panel"
            }
        }

        /// A second block of the file, under a `cfg` of its own.
        #[cfg(not(any()))]
        #[ibis_office_macros::editor_ext(Panel2)]
        impl SlideEditor {
            pub(crate) fn close_panel(&mut self) -> bool {
                self.words.pop().is_some()
            }
        }

        /// A block the `cfg` takes away leaves nothing behind (no trait to import).
        #[cfg(any())]
        #[ibis_office_macros::editor_ext(Panel3)]
        impl SlideEditor {
            pub(crate) fn never_built(&mut self) {}
        }
    }

    /// Another file of the format reaches `pub(super)` methods through the list.
    pub mod title {
        use crate::docx::ext::*;
        use crate::shell::SlideEditor;

        pub fn title(editor: &SlideEditor) -> &'static str {
            editor.panel_title()
        }
    }
}

mod xlsx {
    pub mod ext {
        pub(crate) use super::cells::CellsCrate as _;
    }

    pub mod cells {
        use crate::shell::{Context, SlideEditor};

        // The same names as the document's: each format has its own traits, and a file sees the ones it imports.
        #[ibis_office_macros::editor_ext(Cells)]
        impl SlideEditor {
            pub(crate) fn key_down(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
                self.touch(cx);
                key == "Enter"
            }
        }
    }
}

fn context(notified: &mut u32) -> shell::Context<'_, shell::SlideEditor> {
    shell::Context { notified, entity: std::marker::PhantomData }
}

#[test]
fn the_methods_of_a_format_are_called_as_before() {
    use docx::ext::*;
    let mut notified = 0;
    let mut cx = context(&mut notified);
    let mut editor = shell::SlideEditor::new();
    assert!(editor.key_down("a", &mut cx));
    assert!(!editor.key_down("ab", &mut cx));
    assert_eq!(editor.words, ["one", "two", "three", "panel", "A"]);
    assert_eq!(editor.word_count(), 5);
    assert_eq!(editor.longest(|w| w.len() >= 5), Some("panel"));
    assert_eq!(editor.lengths(2u32).collect::<Vec<_>>(), [6, 6, 10, 10, 2]);
    assert_eq!(editor.platform(), "built");
    assert!(editor.close_panel());
    assert_eq!(docx::title::title(&editor), "panel");
    // touch: once in `key_down`, once in the listener.
    assert_eq!(editor.listener_total(&mut cx), 2 + 5);
    let counter = editor.counter();
    editor.count += 10;
    assert_eq!(counter(), 2);
    assert_eq!(editor.into_words(), ["one", "panel", "three", "two"]);
    // notify: `open_panel` and `touch` in `key_down`, `touch` in the listener.
    assert_eq!(notified, 3);
}

#[test]
fn the_hooks_of_a_format_are_pointers_to_its_methods() {
    let hooks = docx::keys::hooks();
    let mut notified = 0;
    let mut cx = context(&mut notified);
    let mut editor = shell::SlideEditor::new();
    assert!((hooks.key_down)(&mut editor, "b", &mut cx));
    assert_eq!((hooks.word_count)(&editor), 5);
}

#[test]
fn each_format_has_its_own_method_of_a_name() {
    use xlsx::ext::*;
    let mut notified = 0;
    let mut cx = context(&mut notified);
    let mut editor = shell::SlideEditor::new();
    // The workbook's `key_down`: the document's is not imported here.
    assert!(editor.key_down("Enter", &mut cx));
    assert!(!editor.key_down("a", &mut cx));
    assert_eq!(editor.words.len(), 3);
}

/// The compiler's check for code nobody calls reaches the methods of a block (the tokens the macro writes have the place
/// of the name in the attribute). Without the warning, `expect` is not fulfilled and this file does not compile.
#[deny(unfulfilled_lint_expectations)]
mod unused {
    use crate::shell::SlideEditor;

    #[expect(dead_code)]
    mod for_the_crate {
        use super::SlideEditor;

        #[ibis_office_macros::editor_ext(Unused)]
        impl SlideEditor {
            pub(crate) fn nobody_calls_this(&self) {}
        }
    }

    #[expect(dead_code)]
    mod for_the_file {
        use super::SlideEditor;

        #[ibis_office_macros::editor_ext(Unused)]
        impl SlideEditor {
            fn nobody_calls_this_either(&self) {}
        }
    }

    /// A warning in the body of a method is given too.
    mod in_a_body {
        use super::SlideEditor;

        #[ibis_office_macros::editor_ext(Body)]
        impl SlideEditor {
            pub(crate) fn has_a_variable_nobody_reads(&self) -> usize {
                #[expect(unused_variables)]
                let unread = 1;
                self.words.len()
            }
        }

        #[test]
        fn the_method_is_called() {
            assert_eq!(SlideEditor::new().has_a_variable_nobody_reads(), 3);
        }
    }
}
