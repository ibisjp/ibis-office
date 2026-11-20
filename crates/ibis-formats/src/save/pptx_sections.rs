// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Write section names, order and slide membership into the original extension.
use std::collections::{BTreeSet,HashMap};
use ibis_doc_model::Presentation;
use ibis_ooxml::{NodeId,Package,XmlDocument};
use super::{pptx_new::{self as new,Prefixes},unsupported};
use crate::Error;
const S:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
const URI:&str="{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";
fn children(doc:&XmlDocument,parent:NodeId,ns:&str,name:&str)->Vec<NodeId> {
    doc.elements(parent).filter(|&n|doc.namespace(n)==Some(ns)&&doc.local_name(n)==name).collect()
}
fn one(doc:&XmlDocument,parent:NodeId,ns:&str,name:&str)->Result<Option<NodeId>,Error> {
    let nodes=children(doc,parent,ns,name);if nodes.len()>1 {return Err(unsupported("重複するsection構造"));}Ok(nodes.first().copied())
}
fn bindings(doc:&XmlDocument,node:NodeId)->Vec<(Option<String>,String)> {
    let mut prefixes=BTreeSet::new();
    for n in std::iter::once(node).chain(doc.descendants(node)) {
        if !doc.is_element(n) {continue;}
        prefixes.insert(doc.prefix(n).map(str::to_owned));
        for attr in doc.attributes(n) {if let Some((prefix,_))=attr.name.split_once(':') {if prefix!="xmlns" {prefixes.insert(Some(prefix.to_owned()));}}}
    }
    prefixes.into_iter().filter_map(|prefix|doc.resolve_prefix(node,prefix.as_deref()).map(|ns|(prefix,ns.to_owned()))).collect()
}
fn pin(doc:&mut XmlDocument,node:NodeId,bindings:Vec<(Option<String>,String)>) {
    for (prefix,ns) in bindings {if doc.resolve_prefix(node,prefix.as_deref())!=Some(ns.as_str()) {
        let attr=prefix.map(|p|format!("xmlns:{p}")).unwrap_or_else(||"xmlns".to_owned());doc.set_attr(node,&attr,&ns);
    }}
}
fn name(doc:&XmlDocument,parent:NodeId,local:&str)->String {
    let prefix=doc.lookup_prefix(parent,S).expect("section namespace is bound");if prefix.is_empty() {local.to_owned()} else {format!("{prefix}:{local}")}
}
pub(super) fn write(package:&mut Package,main:&str,current:&Presentation)->Result<(),Error> {
    if !current.valid_sections(&current.sections) {return Err(unsupported("セクションのID・所属順"));}
    let doc=package.xml_mut(main)?;let root=doc.root_element().ok_or_else(||Error::new("プレゼンテーションが空です"))?;
    let native=one(doc,root,new::NS_P,"sldIdLst")?.ok_or_else(||unsupported("スライド一覧のないセクション編集"))?;
    let numbers=children(doc,native,new::NS_P,"sldId").into_iter().map(|n|doc.attr(n,"id").and_then(|v|v.parse::<u32>().ok()).ok_or_else(||unsupported("スライドIDのないセクション編集"))).collect::<Result<Vec<_>,_>>()?;
    if numbers.len()!=current.slides.len() {return Err(unsupported("セクションのスライド対応"));}
    let ext=match one(doc,root,new::NS_P,"extLst")? {Some(n)=>n,None=>{if current.sections.is_empty() {return Ok(());}let x=Prefixes::at(doc,root);doc.append_element(root,&format!("{}extLst",x.p))}};
    let extensions=children(doc,ext,new::NS_P,"ext").into_iter().filter(|&n|doc.attr(n,"uri")==Some(URI)).collect::<Vec<_>>();
    if extensions.len()>1 {return Err(unsupported("重複するsection拡張"));}
    let extension=if let Some(&n)=extensions.first() {n} else {if current.sections.is_empty() {return Ok(());}let x=Prefixes::at(doc,ext);let n=doc.append_element(ext,&format!("{}ext",x.p));doc.set_attr(n,"uri",URI);n};
    let list=if let Some(n)=one(doc,extension,S,"sectionLst")? {n} else {
        if current.sections.is_empty() {return Ok(());}
        let n=doc.append_element(extension,"p14:sectionLst");doc.set_attr(n,"xmlns:p14",S);n
    };
    let old=children(doc,list,S,"section");
    if current.sections.is_empty() {
        // An empty sectionLst is invalid. Refuse to discard opaque list data.
        let opaque_attr=doc.attributes(list).iter().any(|a|a.name!="xmlns"&&!a.name.starts_with("xmlns:"));
        let opaque_child=doc.children(list).iter().any(|&n|if doc.is_element(n) {!old.contains(&n)} else {!doc.text(n).trim().is_empty()});
        if opaque_attr || opaque_child {return Err(unsupported("未知情報付きセクション一覧の全削除"));}
        doc.detach(list);return Ok(());
    }
    let mut sections=HashMap::new();let mut members=HashMap::new();
    for &section in &old {
        let id=doc.attr(section,"id").ok_or_else(||unsupported("IDのないセクション編集"))?.to_ascii_uppercase();
        if sections.insert(id,section).is_some() {return Err(unsupported("重複するsection ID"));}
        if let Some(ids)=one(doc,section,S,"sldIdLst")? {
            for node in children(doc,ids,S,"sldId") {
                let number=doc.attr(node,"id").and_then(|v|v.parse::<u32>().ok()).ok_or_else(||unsupported("section所属ID"))?;
                let namespaces=bindings(doc,node);if members.insert(number,(node,namespaces)).is_some() {return Err(unsupported("section所属の重複"));}
            }
        }
    }
    // Capture bindings before moving anything; only known membership nodes go.
    for (node,_) in members.values() {doc.detach(*node);}
    let mut keep=Vec::new();let mut offset=0;
    for section in &current.sections {
        let node=if let Some(&n)=sections.get(&section.id.to_ascii_uppercase()) {n} else {let tag=name(doc,list,"section");doc.append_element(list,&tag)};
        doc.set_attr(node,"id",&section.id);doc.set_attr(node,"name",&section.name);
        let ids=if let Some(n)=one(doc,node,S,"sldIdLst")? {n} else {let tag=name(doc,node,"sldIdLst");doc.append_element(node,&tag)};
        for &number in &numbers[offset..offset+section.slides.len()] {
            let (member,namespaces)=if let Some((n,b))=members.get(&number) {(*n,b.clone())} else {let tag=name(doc,ids,"sldId");let n=doc.append_element(ids,&tag);doc.set_attr(n,"id",&number.to_string());(n,Vec::new())};
            let index=doc.children(ids).len();doc.insert(ids,index,member);pin(doc,member,namespaces);
        }
        offset+=section.slides.len();keep.push(node);
    }
    // Removed section metadata belongs to the section being explicitly removed.
    // List/extension foreign siblings and metadata remain outside that deletion.
    for node in old {doc.detach(node);}
    for node in keep {let index=doc.children(list).len();doc.insert(list,index,node);}
    Ok(())
}
