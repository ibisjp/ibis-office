// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Ephemeral UI preferences owned by one open document, never by its path.
use std::{any::Any, cell::RefCell, collections::HashMap, rc::Rc};
#[derive(Clone, Default)]
pub(crate) struct DocumentMemoryStore(Rc<RefCell<HashMap<&'static str, Box<dyn Any>>>>);
impl DocumentMemoryStore {
    pub(crate) fn get<T: Clone + 'static>(&self, key: &'static str) -> Option<T> {
        self.0.borrow().get(key)?.downcast_ref::<T>().cloned()
    }
    pub(crate) fn set<T: 'static>(&self, key: &'static str, value: T) {
        self.0.borrow_mut().insert(key, Box::new(value));
    }
}
