// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the macro writes and what it refuses, read from its output (tests/editor_ext.rs compiles and runs the output).
use crate::expand::{editor_ext, try_editor_ext};

/// The output without its spaces (the spacing of printed tokens is not something to rely on).
fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn expanded(attr: &str, item: &str) -> String {
    match try_editor_ext(attr.parse().unwrap(), item.parse().unwrap()) {
        Ok(out) => squeeze(&out.to_string()),
        Err(error) => panic!("refused: {error}"),
    }
}

fn refused(attr: &str, item: &str) -> String {
    match try_editor_ext(attr.parse().unwrap(), item.parse().unwrap()) {
        Ok(out) => panic!("not refused: {out}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn each_visibility_gets_its_trait() {
    let out = expanded(
        "Keys",
        "impl SlideEditor {
            pub fn a(&self) {}
            pub(crate) fn b(&self) {}
            pub(super) fn c(&self) {}
            pub(in crate::docx) fn d(&self) {}
            fn e(&self) {}
            pub(crate) fn f(&self) {}
        }",
    );
    for expected in [
        "pubtraitKeys:::core::marker::Sized+'static{fna(&self);}implKeysforSlideEditor{fna(&self){}}",
        "pub(crate)traitKeysCrate:::core::marker::Sized+'static{fnb(&self);fnf(&self);}implKeysCrateforSlideEditor{fnb(&self){}fnf(&self){}}",
        "pub(super)traitKeysSuper:::core::marker::Sized+'static{fnc(&self);}implKeysSuperforSlideEditor{fnc(&self){}}",
        "pub(incrate::docx)traitKeysIn:::core::marker::Sized+'static{fnd(&self);}implKeysInforSlideEditor{fnd(&self){}}",
        "traitKeysPrivate:::core::marker::Sized+'static{fne(&self);}implKeysPrivateforSlideEditor{fne(&self){}}",
    ] {
        assert!(out.contains(expected), "{expected}\nis not in\n{out}");
    }
    // Nothing else: no method is written twice, none is lost.
    assert_eq!(out.matches("trait").count(), 5);
    assert_eq!(out.matches("{}").count(), 6);
}

#[test]
fn the_type_can_be_written_as_a_path() {
    let out = expanded("Keys", "impl crate::shell::editor::SlideEditor { fn a(&self) {} }");
    assert!(out.contains("implKeysPrivateforcrate::shell::editor::SlideEditor{"), "{out}");
}

#[test]
fn an_empty_block_writes_nothing() {
    assert_eq!(expanded("Keys", "impl SlideEditor {}"), "");
}

#[test]
fn the_declaration_takes_no_patterns_and_the_method_keeps_them() {
    let out = expanded("Keys", "impl SlideEditor { fn a(mut self, mut count: i32, (x, y): (i32, i32), ref name: String, _: u8) -> i32 { count += x + y; count } }");
    assert!(out.contains("{fna(self,count:i32,_:(i32,i32),name:String,_:u8)->i32;}"), "{out}");
    assert!(out.contains("{fna(mutself,mutcount:i32,(x,y):(i32,i32),refname:String,_:u8)->i32{count+=x+y;count}}"), "{out}");
}

#[test]
fn generics_and_where_are_carried_to_both() {
    let out = expanded("Keys", "impl SlideEditor { fn a<'a, T: Clone>(&'a self, items: &'a [T], f: impl Fn(&T) -> bool) -> impl Iterator<Item = &'a T> where T: 'a { items.iter().filter(move |i| f(i)) } }");
    let signature = "fna<'a,T:Clone>(&'aself,items:&'a[T],f:implFn(&T)->bool)->implIterator<Item=&'aT>whereT:'a";
    assert!(out.contains(&format!("{signature};")), "{out}");
    assert!(out.contains(&format!("{signature}{{items.iter()")), "{out}");
}

#[test]
fn a_capture_list_names_self_in_the_declaration_only() {
    let out = expanded("Keys", "impl SlideEditor { fn a(&self) -> impl Fn() -> i32 + use<> { || 1 } fn b<T>(&self, t: T) -> impl Sized + use<Self, T> { t } }");
    assert!(out.contains("fna(&self)->implFn()->i32+use<Self>;"), "{out}");
    assert!(out.contains("fna(&self)->implFn()->i32+use<>{||1}"), "{out}");
    assert!(out.contains("fnb<T>(&self,t:T)->implSized+use<Self,T>;"), "{out}");
    let out = expanded("Keys", "impl SlideEditor { fn c<'a, T>(&'a self, t: T) -> impl Sized + use<'a, T> { t } }");
    assert!(out.contains("->implSized+use<'a,Self,T>;"), "{out}");
    assert!(out.contains("->implSized+use<'a,T>{t}"), "{out}");
}

#[test]
fn each_attribute_goes_where_the_compiler_takes_it() {
    let out = expanded(
        "Keys",
        "impl SlideEditor {
            /// What it does.
            #[cfg(test)]
            #[allow(clippy::too_many_arguments)]
            #[inline]
            #[must_use]
            fn a(&self) -> i32 { 1 }
            #[cold]
            #[deprecated]
            #[track_caller]
            fn b(&self) {}
        }",
    );
    // The comment is on the declaration and on the method.
    assert_eq!(out.matches("Whatitdoes.").count(), 2, "{out}");
    assert!(out.contains("#[cfg(test)]#[allow(clippy::too_many_arguments)]#[must_use]fna(&self)->i32;"), "{out}");
    assert!(out.contains("#[cfg(test)]#[allow(clippy::too_many_arguments)]#[inline]fna(&self)->i32{1}"), "{out}");
    assert!(out.contains("#[deprecated]#[track_caller]fnb(&self);"), "{out}");
    assert!(out.contains("#[cold]#[track_caller]fnb(&self){}"), "{out}");
}

#[test]
fn the_attributes_of_the_block_go_on_the_trait_and_on_the_impl() {
    let out = expanded("Keys", "#[cfg(target_os = \"macos\")] impl SlideEditor { fn a(&self) {} }");
    assert!(out.contains("#[cfg(target_os=\"macos\")]traitKeysPrivate"), "{out}");
    assert!(out.contains("#[cfg(target_os=\"macos\")]implKeysPrivatefor"), "{out}");
}

#[test]
fn what_is_not_a_method_is_refused() {
    for (item, expected) in [
        ("impl SlideEditor { const LIMIT: usize = 3; }", "only methods"),
        ("impl SlideEditor { type Item = i32; }", "only methods"),
        ("impl SlideEditor { some_macro!(); }", "only methods"),
        ("impl SlideEditor { pub(crate) const fn a() -> usize { 3 } }", "const fn"),
        ("impl SlideEditor { const unsafe fn a() -> usize { 3 } }", "const fn"),
        ("impl SlideEditor { async fn a(&self) {} }", "`async`, `unsafe` and `extern`"),
        ("impl SlideEditor { pub unsafe fn a(&self) {} }", "`async`, `unsafe` and `extern`"),
        ("impl SlideEditor { extern \"C\" fn a() {} }", "`async`, `unsafe` and `extern`"),
        ("impl SlideEditor { fn a(&self); }", "needs its body"),
        ("impl SlideEditor { #[some_attribute] fn a(&self) {} }", "does not know where this attribute goes"),
        ("impl SlideEditor { #[test] fn a() {} }", "does not know where this attribute goes"),
        ("impl SlideEditor { pub(in crate::a) fn a(&self) {} pub(in crate::b) fn b(&self) {} }", "one `pub(in ..)` path only"),
    ] {
        let error = refused("Keys", item);
        assert!(error.contains(expected), "{item}\n{error}");
    }
}

#[test]
fn what_is_not_a_plain_impl_is_refused() {
    for (item, expected) in [
        ("impl Render for SlideEditor { fn a(&self) {} }", "not on the impl of a trait"),
        ("impl<T> Holder<T> { fn a(&self) {} }", "impl with parameters"),
        ("impl SlideEditor where Self: Sized { fn a(&self) {} }", "`where` on the impl"),
        ("unsafe impl SlideEditor { fn a(&self) {} }", "`unsafe impl`"),
        ("struct SlideEditor;", "goes on `impl Type { fn .. }`"),
        ("fn a() {}", "goes on `impl Type { fn .. }`"),
    ] {
        let error = refused("Keys", item);
        assert!(error.contains(expected), "{item}\n{error}");
    }
}

#[test]
fn the_name_is_one_word() {
    assert!(refused("", "impl SlideEditor {}").contains("is missing"));
    assert!(refused("a::Keys", "impl SlideEditor {}").contains("takes one name"));
    assert!(refused("Keys, More", "impl SlideEditor {}").contains("takes one name"));
    assert!(refused("\"Keys\"", "impl SlideEditor {}").contains("takes one name"));
}

/// A refused block comes out as the error followed by the block as written: the calls of its methods still resolve, and
/// the one error is what the compiler shows.
#[test]
fn a_refused_block_is_kept_beside_the_error() {
    let item = "impl SlideEditor { const LIMIT: usize = 3; fn a(&self) {} }";
    let out = squeeze(&editor_ext("Keys".parse().unwrap(), item.parse().unwrap()).to_string());
    assert!(out.starts_with("::core::compile_error!{\"#[editor_ext]:onlymethods"), "{out}");
    assert!(out.ends_with(&squeeze(item)), "{out}");
}
