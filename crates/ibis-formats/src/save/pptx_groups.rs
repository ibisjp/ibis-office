// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reuse native nodes when changing the group forest. New groups are neutral
//! identity containers; removing non-neutral or opaque ancestors is refused.
use std::collections::{BTreeSet,HashMap,HashSet};
use ibis_doc_model::{GroupMember,ShapeGroup,ShapeId,Slide};
use ibis_ooxml::{NodeId,XmlDocument};
use super::{pptx_new::{self as new,Prefixes},unsupported};
use crate::Error;
fn known(doc:&XmlDocument,node:NodeId,ns:&str,name:&str)->bool {doc.is_element(node)&&doc.namespace(node)==Some(ns)&&doc.local_name(node)==name}
fn child(doc:&XmlDocument,parent:NodeId,ns:&str,name:&str)->Result<NodeId,Error> {
    let nodes=doc.elements(parent).filter(|&n|known(doc,n,ns,name)).collect::<Vec<_>>();if nodes.len()!=1 {return Err(unsupported("グループの既知構造が一意でない"));}Ok(nodes[0])
}
fn meta(doc:&XmlDocument,node:NodeId)->Result<NodeId,Error> {let nv=child(doc,node,new::NS_P,"nvGrpSpPr")?;child(doc,nv,new::NS_P,"cNvPr")}
fn clean_attrs(doc:&XmlDocument,node:NodeId,allow:&[&str])->bool {doc.attributes(node).iter().all(|a|a.name=="xmlns"||a.name.starts_with("xmlns:")||allow.contains(&a.name.as_str()))}
fn clean_children(doc:&XmlDocument,node:NodeId,allow:&[NodeId])->bool {doc.children(node).iter().all(|&n|if doc.is_element(n) {allow.contains(&n)} else {doc.kind(n)==ibis_ooxml::xml::NodeKind::Text&&doc.text(n).trim().is_empty()})}
/// Only a group with no inherited style, turn, scale or opaque group data can
/// disappear or enter/leave a node's ancestor chain without materialization.
fn neutral(doc:&XmlDocument,node:NodeId)->bool {
    fn check(doc:&XmlDocument,node:NodeId)->Result<bool,Error> {
        let nv=child(doc,node,new::NS_P,"nvGrpSpPr")?;let pr=child(doc,node,new::NS_P,"grpSpPr")?;
        let shapes=doc.elements(node).filter(|&n|doc.namespace(n)==Some(new::NS_P)&&matches!(doc.local_name(n),"sp"|"cxnSp"|"pic"|"graphicFrame"|"grpSp")).collect::<Vec<_>>();let mut allowed=vec![nv,pr];allowed.extend(shapes);
        if !clean_attrs(doc,node,&[])||!clean_children(doc,node,&allowed) {return Ok(false);}
        let c=child(doc,nv,new::NS_P,"cNvPr")?;let g=child(doc,nv,new::NS_P,"cNvGrpSpPr")?;let p=child(doc,nv,new::NS_P,"nvPr")?;
        if !clean_attrs(doc,nv,&[])||!clean_children(doc,nv,&[c,g,p])||!clean_attrs(doc,c,&["id","name"])||!clean_children(doc,c,&[])||!clean_attrs(doc,g,&[])||!clean_children(doc,g,&[])||!clean_attrs(doc,p,&[])||!clean_children(doc,p,&[]) {return Ok(false);}
        let x=child(doc,pr,new::NS_A,"xfrm")?;if !clean_attrs(doc,pr,&[])||!clean_children(doc,pr,&[x])||!clean_attrs(doc,x,&["rot","flipH","flipV"]) {return Ok(false);}
        if doc.attr(x,"rot").is_some_and(|v|v!="0")||["flipH","flipV"].iter().any(|&key|doc.attr(x,key).is_some_and(|v|v!="0"&&v!="false")) {return Ok(false);}
        let o=child(doc,x,new::NS_A,"off")?;let e=child(doc,x,new::NS_A,"ext")?;let co=child(doc,x,new::NS_A,"chOff")?;let ce=child(doc,x,new::NS_A,"chExt")?;
        if !clean_children(doc,x,&[o,e,co,ce]) {return Ok(false);}
        for (a,b,keys) in [(o,co,["x","y"]),(e,ce,["cx","cy"])] {if !clean_attrs(doc,a,&keys)||!clean_attrs(doc,b,&keys)||!clean_children(doc,a,&[])||!clean_children(doc,b,&[]) {return Ok(false);}for key in keys {let value=doc.attr(a,key).and_then(|v|v.parse::<i64>().ok());if value.is_none()||value!=doc.attr(b,key).and_then(|v|v.parse::<i64>().ok()) {return Ok(false);}}}
        Ok(true)
    }
    check(doc,node).unwrap_or(false)
}
fn bindings(doc:&XmlDocument,node:NodeId)->Vec<(Option<String>,String)> {
    let mut prefixes=BTreeSet::new();for n in std::iter::once(node).chain(doc.descendants(node)) {if !doc.is_element(n) {continue;}prefixes.insert(doc.prefix(n).map(str::to_owned));for a in doc.attributes(n) {if let Some((p,_))=a.name.split_once(':') {if p!="xmlns" {prefixes.insert(Some(p.to_owned()));}}}}
    prefixes.into_iter().filter_map(|p|doc.resolve_prefix(node,p.as_deref()).map(|ns|(p,ns.to_owned()))).collect()
}
fn pin(doc:&mut XmlDocument,node:NodeId,bindings:Vec<(Option<String>,String)>) {for (p,ns) in bindings {if doc.resolve_prefix(node,p.as_deref())!=Some(ns.as_str()) {let key=p.map(|p|format!("xmlns:{p}")).unwrap_or_else(||"xmlns".into());doc.set_attr(node,&key,&ns);}}}
fn ancestors(groups:&[ShapeGroup])->(HashMap<String,Vec<String>>,HashMap<ShapeId,Vec<String>>) {
    fn visit(g:&ShapeGroup,parents:&[String],gs:&mut HashMap<String,Vec<String>>,ss:&mut HashMap<ShapeId,Vec<String>>) {gs.insert(g.native_id.clone(),parents.to_vec());let mut next=parents.to_vec();next.push(g.native_id.clone());for c in &g.children {match c {GroupMember::Shape(id)=>{ss.insert(*id,next.clone());},GroupMember::Group(g)=>visit(g,&next,gs,ss)}}}
    let mut gs=HashMap::new();let mut ss=HashMap::new();for g in groups {visit(g,&[],&mut gs,&mut ss);} (gs,ss)
}
fn specs<'a>(groups:&'a[ShapeGroup],out:&mut HashMap<String,&'a ShapeGroup>) {for g in groups {out.insert(g.native_id.clone(),g);for c in &g.children {if let GroupMember::Group(g)=c {specs(std::slice::from_ref(g.as_ref()),out);}}}}
fn first(g:&ShapeGroup,slide:&Slide)->usize {g.children.iter().filter_map(|c|match c {GroupMember::Shape(id)=>slide.shape_index(*id),GroupMember::Group(g)=>Some(first(g,slide))}).min().unwrap_or(usize::MAX)}
fn group_xml(x:&Prefixes,g:&ShapeGroup,slide:&Slide)->Result<String,Error> {
    fn leaves(g:&ShapeGroup,out:&mut Vec<ShapeId>) {for c in &g.children {match c {GroupMember::Shape(id)=>out.push(*id),GroupMember::Group(g)=>leaves(g,out)}}}
    let mut ids=Vec::new();leaves(g,&mut ids);let mut bounds:Option<(i64,i64,i64,i64)>=None;
    for id in ids {let f=slide.shape(id).ok_or_else(||unsupported("グループの図形がない"))?.frame;let r=f.x.checked_add(f.width).ok_or_else(||unsupported("グループ座標範囲"))?;let b=f.y.checked_add(f.height).ok_or_else(||unsupported("グループ座標範囲"))?;if f.width<0||f.height<0 {return Err(unsupported("負のグループ図形サイズ"));}bounds=Some(match bounds {None=>(f.x,f.y,r,b),Some((l,t,right,bottom))=>(l.min(f.x),t.min(f.y),right.max(r),bottom.max(b))});}
    let (l,t,r,b)=bounds.unwrap_or((0,0,1,1));let w=r.checked_sub(l).ok_or_else(||unsupported("グループ座標範囲"))?.max(1);let h=b.checked_sub(t).ok_or_else(||unsupported("グループ座標範囲"))?.max(1);let p=&x.p;let a=&x.a;let name=new::escape(&g.name);let id=&g.native_id;
    Ok(format!("<{p}grpSp><{p}nvGrpSpPr><{p}cNvPr id=\"{id}\" name=\"{name}\"/><{p}cNvGrpSpPr/><{p}nvPr/></{p}nvGrpSpPr><{p}grpSpPr><{a}xfrm><{a}off x=\"{l}\" y=\"{t}\"/><{a}ext cx=\"{w}\" cy=\"{h}\"/><{a}chOff x=\"{l}\" y=\"{t}\"/><{a}chExt cx=\"{w}\" cy=\"{h}\"/></{a}xfrm></{p}grpSpPr></{p}grpSp>"))
}
/// Replace only the known painting slots. Opaque siblings retain their order.
fn install(doc:&mut XmlDocument,parent:NodeId,old:&[NodeId],known:&HashSet<NodeId>,desired:&[NodeId]) {
    let mut result=Vec::new();let mut at=0;let mut last=None;
    for &node in old {if known.contains(&node) {if let Some(&n)=desired.get(at) {result.push(n);at+=1;last=Some(result.len());}} else {result.push(node);}}
    let tail=last.unwrap_or_else(||result.iter().position(|&n|doc.is_element(n)&&doc.local_name(n)=="extLst").unwrap_or(result.len()));result.splice(tail..tail,desired[at..].iter().copied());
    for (index,node) in result.into_iter().enumerate() {doc.insert(parent,index,node);}
}
pub(super) fn write(doc:&mut XmlDocument,tree:NodeId,slide:&Slide,old:&[ShapeGroup],leaves:&HashMap<ShapeId,NodeId>)->Result<(),Error> {
    if !slide.valid_groups(&slide.groups) {return Err(unsupported("グループ階層のIDと所属"));}
    let mut old_specs=HashMap::new();specs(old,&mut old_specs);let mut desired_specs=HashMap::new();specs(&slide.groups,&mut desired_specs);
    fn original_groups(doc:&XmlDocument,parent:NodeId,out:&mut HashMap<String,NodeId>)->Result<(),Error> {
        const MC:&str="http://schemas.openxmlformats.org/markup-compatibility/2006";
        for n in doc.elements(parent) {if known(doc,n,new::NS_P,"grpSp") {let c=meta(doc,n)?;let id=doc.attr(c,"id").ok_or_else(||unsupported("グループIDがない"))?.to_owned();if out.insert(id,n).is_some() {return Err(unsupported("重複する原本group ID"));}original_groups(doc,n,out)?;} else if known(doc,n,MC,"AlternateContent") {if let Some(branch)=doc.elements(n).find(|&n|known(doc,n,MC,"Choice")).or_else(||doc.elements(n).find(|&n|known(doc,n,MC,"Fallback"))) {original_groups(doc,branch,out)?;}}}Ok(())
    }
    let mut original=HashMap::new();original_groups(doc,tree,&mut original)?;
    for id in old_specs.keys() {if !original.contains_key(id) {return Err(unsupported("原本groupが見つからない"));}}
    for (id,g) in &desired_specs {match old_specs.get(id) {Some(old) if old.has_unmodelled_children!=g.has_unmodelled_children=>return Err(unsupported("未読込group子の情報変更")),None if g.has_unmodelled_children=>return Err(unsupported("原本のない未読込group子")),_=>{}}}
    fn same_structure(old:&[ShapeGroup],new:&[ShapeGroup])->bool {old.len()==new.len()&&old.iter().zip(new).all(|(a,b)|a.native_id==b.native_id&&a.has_unmodelled_children==b.has_unmodelled_children&&a.children.len()==b.children.len()&&a.children.iter().zip(&b.children).all(|(a,b)|match (a,b) {(GroupMember::Shape(a),GroupMember::Shape(b))=>a==b,(GroupMember::Group(a),GroupMember::Group(b))=>same_structure(std::slice::from_ref(a.as_ref()),std::slice::from_ref(b.as_ref())),_=>false}))}
    if same_structure(old,&slide.groups) {for (id,g) in desired_specs {let c=meta(doc,original[&id])?;doc.set_attr(c,"name",&g.name);}return Ok(());}
    let tracked=leaves.values().copied().chain(original.values().copied()).collect::<HashSet<_>>();
    let neutral=original.iter().map(|(id,&n)|(id.clone(),neutral(doc,n)&&doc.elements(n).filter(|&n|doc.namespace(n)==Some(new::NS_P)&&matches!(doc.local_name(n),"sp"|"cxnSp"|"pic"|"graphicFrame"|"grpSp")).all(|n|tracked.contains(&n)))).collect::<HashMap<_,_>>();
    for &node in original.values().chain(leaves.values()) {if let Some(parent)=doc.parent(node) {if parent!=tree&&!original.values().any(|&n|n==parent) {return Err(unsupported("MC分岐などの包みを保つグループ階層変更"));}}}
    for id in old_specs.keys().filter(|id|!desired_specs.contains_key(*id)) {if !neutral.get(id).copied().unwrap_or(false) {return Err(unsupported("変換・継承・未知情報付きグループの解除"));}}
    let (og,os)=ancestors(old);let (ng,ns)=ancestors(&slide.groups);
    let opaque_chain=|chain:&[String]|chain.iter().filter(|id|original.contains_key(*id)&&!neutral.get(*id).copied().unwrap_or(false)).cloned().collect::<Vec<_>>();
    for (id,chain) in &ng {if let Some(before)=og.get(id) {if opaque_chain(before)!=opaque_chain(chain) {return Err(unsupported("原本group親の変換・継承変更"));}}}
    for shape in &slide.shapes {let before=os.get(&shape.id).map(Vec::as_slice).unwrap_or(&[]);let after=ns.get(&shape.id).map(Vec::as_slice).unwrap_or(&[]);if opaque_chain(before)!=opaque_chain(after) {return Err(unsupported("原本図形のgroup親の変換・継承変更"));}if !leaves.contains_key(&shape.id)&&!slide.is_attached(shape.id) {return Err(unsupported("グループ編集できない図形"));}}
    // Drawing IDs are shared by the slide's known nonvisual properties.
    let ids=doc.descendants(tree).into_iter().filter(|&n|known(doc,n,new::NS_P,"cNvPr")).filter_map(|n|doc.attr(n,"id").and_then(|v|v.parse::<u32>().ok())).collect::<HashSet<_>>();
    for id in desired_specs.keys().filter(|id|!original.contains_key(*id)) {if ids.contains(&id.parse::<u32>().map_err(|_|unsupported("group ID"))?) {return Err(unsupported("図形と衝突するgroup ID"));}}
    let root_old=doc.children(tree).to_vec();let mut group_old=HashMap::new();let mut saved=Vec::new();let mut known_nodes=leaves.values().copied().collect::<HashSet<_>>();for (&id,&node) in leaves {let _=id;saved.push((node,bindings(doc,node)));}
    for (id,&node) in &original {known_nodes.insert(node);group_old.insert(id.clone(),doc.children(node).to_vec());saved.push((node,bindings(doc,node)));}
    let x=Prefixes::at(doc,tree);let mut handles=original.clone();
    fn make(doc:&mut XmlDocument,x:&Prefixes,groups:&[ShapeGroup],slide:&Slide,handles:&mut HashMap<String,NodeId>)->Result<(),Error> {for g in groups {if !handles.contains_key(&g.native_id) {let node=x.import(doc,&group_xml(x,g,slide)?)?;for (prefix,ns) in [(&x.p,new::NS_P),(&x.a,new::NS_A)] {let key=prefix.strip_suffix(':').map(|p|format!("xmlns:{p}")).unwrap_or_else(||"xmlns".to_owned());doc.set_attr(node,&key,ns);}handles.insert(g.native_id.clone(),node);}for c in &g.children {if let GroupMember::Group(g)=c {make(doc,x,std::slice::from_ref(g.as_ref()),slide,handles)?;}}}Ok(())}
    make(doc,&x,&slide.groups,slide,&mut handles)?;
    for &n in original.values() {doc.detach(n);}for &n in leaves.values() {doc.detach(n);}
    for (node,b) in &saved {pin(doc,*node,b.clone());}
    fn fill(doc:&mut XmlDocument,g:&ShapeGroup,handles:&HashMap<String,NodeId>,leaves:&HashMap<ShapeId,NodeId>,old:&HashMap<String,Vec<NodeId>>,known:&HashSet<NodeId>)->Result<(),Error> {
        let node=handles[&g.native_id];let mut desired=Vec::new();for c in &g.children {desired.push(match c {GroupMember::Shape(id)=>*leaves.get(id).ok_or_else(||unsupported("group所属の実ノードがない"))?,GroupMember::Group(g)=>{fill(doc,g,handles,leaves,old,known)?;handles[&g.native_id]}});}
        let children=old.get(&g.native_id).cloned().unwrap_or_else(||doc.children(node).to_vec());install(doc,node,&children,known,&desired);let c=meta(doc,node)?;doc.set_attr(c,"name",&g.name);Ok(())
    }
    for g in &slide.groups {fill(doc,g,&handles,leaves,&group_old,&known_nodes)?;}
    let mut root_by_shape=HashMap::new();fn assign(g:&ShapeGroup,root:NodeId,out:&mut HashMap<ShapeId,NodeId>) {for c in &g.children {match c {GroupMember::Shape(id)=>{out.insert(*id,root);},GroupMember::Group(g)=>assign(g,root,out)}}}for g in &slide.groups {assign(g,handles[&g.native_id],&mut root_by_shape);}
    let mut roots=Vec::new();for shape in &slide.shapes {if slide.is_attached(shape.id) {continue;}let n=root_by_shape.get(&shape.id).copied().or_else(||leaves.get(&shape.id).copied()).ok_or_else(||unsupported("group一覧の実ノードがない"))?;if roots.last()!=Some(&n) {roots.push(n);}}
    if roots.iter().copied().collect::<HashSet<_>>().len()!=roots.len() {return Err(unsupported("連続しないgroupの所属順"));}
    for (index,g) in slide.groups.iter().enumerate().filter(|(_,g)|first(g,slide)==usize::MAX) {let next=slide.groups[index+1..].iter().find(|g|first(g,slide)!=usize::MAX).map(|g|handles[&g.native_id]);let at=next.and_then(|n|roots.iter().position(|&x|x==n)).unwrap_or(roots.len());roots.insert(at,handles[&g.native_id]);}
    install(doc,tree,&root_old,&known_nodes,&roots);
    for (node,bindings) in saved {if doc.parent(node).is_some() {pin(doc,node,bindings);}}
    Ok(())
}
