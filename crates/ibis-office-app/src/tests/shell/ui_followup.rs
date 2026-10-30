// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario,button_probe as buttons};
use crate::docx::scenarios::DocSteps;
use gpui_kit::{*,component::input::InputEvent};
pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
 ("shell::shape_decoration_live_and_save",effects),
 ("shell::generic_cell_picker_commit_cancel",cell_picker),
 ("shell::sort_level_editor_cancel",sort_level),
 ("shell::shared_tab_modes_and_wheel",tab_modes),
 ("shell::dialog_action_bar_uniform_insets",action_bar_insets),
 ("shell::color_palette_wraps_within_popup",palette_wrap),
 ("shell::shared_theme_palette_editor",theme_palette),
 ("shell::submenu_outside_parent_and_explicit_narrow",submenu_cascade_review::run),
]}
pub(crate) fn ignored()->Vec<(&'static str,fn())>{vec![]}
fn sidebar_shot(s:&mut Scenario,name:&str){
 let path=s.screenshot(name);let img=image::open(path).unwrap().to_rgba8();let scale=img.width()/1200;
 let pane=image::imageops::crop_imm(&img,img.width()-300*scale,108*scale,300*scale,img.height()-(108+28)*scale).to_image();
 let root=crate::shell::scenario::repo_root().join("target/ui-followup-extras");std::fs::create_dir_all(&root).unwrap();pane.save(root.join(format!("{name}.png"))).unwrap();
}
fn effects(){
 let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,900.);
 let(id,_)=s.paragraph(0);s.click_text(ibis_doc_model::FlowPos::new(id,0));s.menu("insert.text-box");
 let tab=buttons::all().into_iter().find(|k|k.contains("text-box-tabs")&&k.ends_with(", 1)")).expect("decoration tab");buttons::click(&mut s,&tab);
 assert!(s.with(|e,_|e.doc_state().unwrap().text_box.as_ref().unwrap().effects.open.iter().all(|b|!*b)));
 sidebar_shot(&mut s,"shape-decoration-collapsed");
 for index in 0..5{
  s.update(|e,_,cx|{let x=e.doc_state_mut().unwrap().text_box.as_mut().unwrap();x.effects.open=[false;5];x.effects.open[index]=true;cx.notify();});
  let key=buttons::all().into_iter().find(|k|k.contains("shape-effect-enable")&&k.ends_with(&format!(", {index})"))).expect("enabled checkbox");buttons::click(&mut s,&key);
  if index==0 {sidebar_shot(&mut s,"shape-decoration-gradient");}
 }
 s.update(|e,w,cx|{let field=e.doc_state().unwrap().text_box.as_ref().unwrap().effects.fields["shadow-distance"].clone();field.update(cx,|i,cx|{i.set_value("12",w,cx);cx.emit(InputEvent::Change);});});
 let info=s.with(|e,_|e.text_box_at_caret().unwrap());assert!(matches!(info.effects.fill,ibis_doc_model::Fill::Linear{..}));assert_eq!(info.effects.shadow.as_ref().unwrap().distance,12*12700);assert!(info.effects.glow.is_some()&&info.effects.soft.is_some()&&info.effects.reflection.is_some());
 s.update(|e,_,cx|{let x=e.doc_state_mut().unwrap().text_box.as_mut().unwrap();x.effects.open=[false;5];x.effects.open[1]=true;cx.notify();});
 sidebar_shot(&mut s,"shape-decoration-shadow-live");
 s.save_as("decoration-live.docx");let reloaded=s.reopen();
 let at=info.at;let effects=reloaded.flow.as_ref().unwrap().body.iter().find_map(|b|if let ibis_doc_model::Block::Paragraph(p)=b{if p.id==at.para{p.runs.iter().find_map(|r|if let ibis_doc_model::FlowRunKind::Shape(shape)=&r.kind{if let ibis_doc_model::ShapeKind::Auto(a)=&shape.kind{Some(a)}else{None}}else{None})}else{None}}else{None});
 assert!(effects.is_some(),"saved shape survives reopen");
 let saved=effects.unwrap();assert_eq!(saved.fill,info.effects.fill);assert_eq!(saved.shadow,info.effects.shadow);assert_eq!(saved.glow,info.effects.glow);assert_eq!(saved.soft_edge,info.effects.soft);assert_eq!(saved.reflection,info.effects.reflection);
}
fn cell_picker(){
 let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,800.);
 s.update(|e,w,cx|e.open_goal_seek(w,cx));
 let original=s.with(|e,cx|e.wb().panels.goal_seek.as_ref().unwrap().formula.read(cx).value().to_string());
 buttons::click(&mut s,"goal_seek-p-formula");assert!(s.with(|e,_|e.sheet_range_pick_active()));
 let p=s.with(|e,_|e.cell_center(2,4).unwrap());s.click((f32::from(p.x),f32::from(p.y)));
 assert!(!s.with(|e,_|e.sheet_range_pick_active()));assert_eq!(s.with(|e,cx|e.wb().panels.goal_seek.as_ref().unwrap().formula.read(cx).value().to_string()),"C5");
 assert_ne!(original,"C5");
 s.update(|e,w,cx|e.open_dedupe_panel(w,cx));
 let input=s.with(|e,_|e.wb().panels.dedupe.as_ref().unwrap().range.clone());let original=s.with(|_,cx|input.read(cx).value().to_string());
 buttons::click(&mut s,"dedupe-range");let p=s.with(|e,_|e.cell_center(1,4).unwrap());s.click((f32::from(p.x),f32::from(p.y)));assert!(s.with(|e,_|e.sheet_range_pick_active()));
 s.key("escape");assert_eq!(s.with(|_,cx|input.read(cx).value().to_string()),original);assert!(!s.with(|e,_|e.sheet_range_pick_active()));
}

fn sort_level(){
 let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,800.);s.update(|e,w,cx|e.open_sort_panel(w,cx));
 let original=s.with(|e,cx|e.wb().sort_edit.as_ref().unwrap().levels.iter().map(|(f,a)|(f.read(cx).value().to_string(),*a)).collect::<Vec<_>>());
 buttons::click(&mut s,"sort-add");assert!(s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().editor.is_some()));
 buttons::click(&mut s,"sort-level-cancel");assert_eq!(s.with(|e,cx|e.wb().sort_edit.as_ref().unwrap().levels.iter().map(|(f,a)|(f.read(cx).value().to_string(),*a)).collect::<Vec<_>>()),original);
 let key=buttons::all().into_iter().find(|k|k.contains("sort-edit")&&k.ends_with(", 0)")).unwrap();buttons::click(&mut s,&key);
 s.key("escape");assert!(s.with(|e,_|e.wb().sort_edit.as_ref().unwrap().editor.is_none()));
 s.screenshot("sort-levels-table");
}

fn tab_fixture(_: &mut crate::shell::editor::SlideEditor,_:&mut Window,cx:&mut Context<crate::shell::editor::SlideEditor>)->Option<crate::shell::chrome::SidebarPanel>{
 use crate::shell::chrome::{self,TabMode};
 let row=|id,mode,labels:Vec<SharedString>,width|chrome::tab_control_with_mode(labels,0,id,mode,|_,_,_|{},cx).w(px(width));
 Some(chrome::SidebarPanel{title:"Tabs".into(),body:gpui_kit::component::v_flex().gap_3()
   .child(row("tabs-equal",TabMode::Equal,vec!["A".into(),"BBBBB".into()],200.))
   .child(row("tabs-intrinsic",TabMode::Intrinsic,vec!["A".into(),"BBBBB".into()],200.))
   .child(row("tabs-fallback",TabMode::Equal,vec!["A".into(),"Long dictionary settings".into()],120.)).into_any_element()})
}
fn tab_modes(){
 struct Reset;impl Drop for Reset{fn drop(&mut self){crate::shell::chrome::FORCED_SIDEBAR.with(|s|s.set(None));}}
 let _reset=Reset;let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,800.);
 crate::shell::chrome::FORCED_SIDEBAR.with(|slot|slot.set(Some((tab_fixture,"tab-mode-test",crate::shell::panel_dialog::PanelMode::Sidebar))));s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
 let bounds=|id:&str,index|{let key=buttons::all().into_iter().find(|k|k.contains(id)&&k.ends_with(&format!(", {index})"))).unwrap();buttons::bounds(&key).unwrap()};
 assert_eq!(bounds("tabs-equal",0).size.width,bounds("tabs-equal",1).size.width);
 assert!(bounds("tabs-intrinsic",0).size.width<bounds("tabs-intrinsic",1).size.width);
 assert!(bounds("tabs-fallback",0).size.width<bounds("tabs-fallback",1).size.width);
 let before=bounds("tabs-fallback",0);s.wheel((f32::from(before.center().x),f32::from(before.center().y)),-120.);let after=bounds("tabs-fallback",0);assert!(after.origin.x<before.origin.x,"vertical wheel scrolls overflowing tabs horizontally");
 s.screenshot("tab-layout-modes");
}

fn action_bar_insets(){
 let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
 s.update(|e,_,cx|e.open_update_toc_panel(cx));
 check_action_bar(&mut s,"toc-update-ok","update-toc");
 s.update(|e,w,cx|{e.close_update_toc_panel(cx);e.open_user_info_panel(w,cx);});
 check_action_bar(&mut s,"user-info-apply","user-info");
 drop(s);let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,800.);s.update(|e,w,cx|e.open_sparkline_panel(None,w,cx));check_action_bar(&mut s,"xlsx.sparkline.apply","sparkline");
 s.update(|e,_,cx|e.open_cells_dialog(true,cx));check_action_bar(&mut s,"cells-ok","insert-cells");
 s.update(|e,_,cx|e.open_cells_dialog(false,cx));check_action_bar(&mut s,"cells-ok","delete-cells");
}
fn check_action_bar(s:&mut Scenario,id:&str,name:&str){
 let button=buttons::bounds(id).expect("primary action");let(frame,footer)=s.with(|e,_|(e.panel_dialog.bounds.unwrap(),e.panel_dialog.footer_height));
 let bottom=frame.bottom()-px(1.);let top=bottom-footer;
 let above=f32::from(button.origin.y-top);let below=f32::from(bottom-button.bottom());
 let expected=crate::shell::chrome::ACTION_BAR_PADDING_Y;
 assert!((above-expected).abs()<=1. && (below-expected).abs()<=1.,"{name}: footer top={above}, bottom={below}, expected={expected}");
 let(width,bytes)=s.picture();let height=bytes.len() as u32/(width*4);let image=image::RgbaImage::from_raw(width,height,bytes).unwrap();let scale=width as f32/1200.;
 let crop=image::imageops::crop_imm(&image,(f32::from(frame.origin.x)*scale).round() as u32,(f32::from(frame.origin.y)*scale).round() as u32,(f32::from(frame.size.width)*scale).round() as u32,(f32::from(frame.size.height)*scale).round() as u32).to_image();
 let root=crate::shell::scenario::repo_root().join("target/action-bar-review");std::fs::create_dir_all(&root).unwrap();crop.save(root.join(format!("{name}.png"))).unwrap();
}

fn palette_wrap(){
 struct Reset; impl Drop for Reset{fn drop(&mut self){crate::shell::chrome::FORCED_SIDEBAR.with(|v|v.set(None));}} let _reset=Reset;
 use crate::shell::chrome::{self,SidebarPanel,FORCED_SIDEBAR};
 thread_local!{static FIELD:std::cell::Cell<Option<Bounds<Pixels>>>=const{std::cell::Cell::new(None)};}
 fn panel(_: &mut crate::shell::editor::SlideEditor,w:&mut Window,cx:&mut Context<crate::shell::editor::SlideEditor>)->Option<SidebarPanel>{
  let input=cx.new(|cx|gpui_kit::component::input::InputState::new(w,cx).default_value("000000"));
  let body=div().relative().w(px(124.)).child(chrome::panel_color_field("palette-wrap-test",&input,cx))
   .child(canvas(|_,_,_|(),|b,_,_,_|FIELD.with(|v|v.set(Some(b)))).absolute().inset_0());
  Some(SidebarPanel::with_footer("Palette test",body.into_any_element(),chrome::action_bar().action(chrome::dialog_close_definition(cx))))
 }
 let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(800.,600.);
 FORCED_SIDEBAR.with(|v|v.set(Some((panel,"palette-wrap-test",crate::shell::panel_dialog::PanelMode::Modal))));
 s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
 let b=FIELD.with(|v|v.get().unwrap());s.click((f32::from(b.origin.x)+16.,f32::from(b.center().y)));
 let colors=crate::shell::color_editor::palette_colors();assert!(colors.len()>10);
 let first=buttons::bounds("palette-standard-0").expect("palette opened");
 let mut wrapped=false;
 for i in 0..colors.len(){let b=buttons::bounds(&format!("palette-standard-{i}")).expect("swatch measured");assert!(f32::from(b.left())>=0.&&f32::from(b.right())<=800.,"palette swatch {i} extends outside viewport: {b:?}");wrapped|=b.top()>first.top();let border=if i==0{2.}else{1.};assert!((f32::from(b.size.width)+2.*border-18.).abs()<0.1,"swatch {i} keeps its 18px outer size: {b:?}");}
 assert!(wrapped,"a long palette wraps rather than extending in one row");
 FORCED_SIDEBAR.with(|v|v.set(None));
}
fn theme_palette(){
 let _probe=buttons::Probe::new();
 for file in ["test/editing/docx-report.docx","test/editing/xlsx-sales.xlsx","test/editing/pptx-plan.pptx"]{
  let mut s=Scenario::open(file);s.size(1200.,800.);
  s.update(|e,w,cx|{if e.is_document(){e.open_docx_theme_panel(w,cx);}else if e.presentation().workbook.is_some(){e.open_xlsx_theme_panel(w,cx);}else{e.open_theme_panel(w,cx);}});
  assert!(buttons::bounds("theme-add").is_some(),"all formats use the shared palette table");
  buttons::click(&mut s,"theme-add");assert!(s.with(|e,_|e.theme_colors_ui.editing));
  buttons::click(&mut s,"theme-custom-cancel");assert!(!s.with(|e,_|e.theme_colors_ui.editing));
  assert!(buttons::bounds("theme-add").is_some(),"Cancel returns to the palette list");
  buttons::click(&mut s,"theme-add");
  let field=s.with(|e,_|if e.is_document(){e.doc_state().unwrap().theme_panel.as_ref().unwrap().colors[0].2.clone()}else if e.presentation().workbook.is_some(){e.wb().panels.theme.as_ref().unwrap().colors[0].2.clone()}else{e.pptx().unwrap().theme_panel.as_ref().unwrap().colors[0].2.clone()});
  s.update(|_,w,cx|field.update(cx,|i,cx|i.set_value("001122",w,cx)));
  buttons::click(&mut s,"theme-custom-apply");
  assert!(!s.with(|e,_|e.theme_colors_ui.editing));
  assert!(s.with(|e,_|{let colors=e.presentation().workbook.as_ref().map(|w|&w.theme_colors).unwrap_or(&e.presentation().theme_colors);colors.iter().any(|(_,c)|c.0==0x001122)}),"custom colors are committed before returning to the list");
 }
}


/// Integration coverage for the public popup API, including its real mouse hitboxes and dismiss chain.
mod submenu_cascade_review {
    use super::{Scenario, buttons};
    use crate::shell::{chrome::{self, FORCED_SIDEBAR, SidebarPanel}, editor::SlideEditor};
    use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
    use gpui_kit::*;
    use std::{cell::{Cell, RefCell}, rc::Rc};

    struct State {
        case: usize,
        popup: Option<Entity<PopupMenu>>,
        parent: Rc<Cell<Option<Bounds<Pixels>>>>,
        invoked: Rc<Cell<usize>>,
        dismissed: Rc<Cell<usize>>,
        subscription: Option<Subscription>,
    }
    thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }
    struct Reset(Option<chrome::CapturedPanel>);
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCED_SIDEBAR.with(|slot| slot.set(self.0));
            STATE.with(|slot| { slot.borrow_mut().take(); });
        }
    }
    fn panel(_: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
        let (popup, measured, position) = STATE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let state = slot.as_mut().expect("cascade scenario installed");
            let (x, parent_width, child_width) = match state.case {
                0 => (40., 180., 240.),
                1 => (640., 220., 300.),
                _ => (250., 300., 500.),
            };
            if state.popup.is_none() {
                let invoked = state.invoked.clone();
                let popup = PopupMenu::build(window, cx, move |menu, w, cx| {
                    let invoked = invoked.clone();
                    menu.min_w(px(parent_width)).max_w(px(parent_width)).scrollable(false)
                        .submenu("More", w, cx, move |child, _, _| {
                            let invoked = invoked.clone();
                            child.min_w(px(child_width)).max_w(px(child_width)).scrollable(false)
                                .item(PopupMenuItem::element(|_, _| {
                                    // The button only supplies a probe and visible content. PopupMenu owns activation/dismissal.
                                    chrome::action_button("cascade-review-leaf").label("Execute").w_full()
                                }).on_click(move |_, _, _| invoked.set(invoked.get() + 1)))
                        })
                });
                let dismissed = state.dismissed.clone();
                state.subscription = Some(cx.subscribe_in(&popup, window, move |_, _, _: &DismissEvent, _, _| dismissed.set(dismissed.get() + 1)));
                popup.focus_handle(cx).focus(window, cx);
                state.popup = Some(popup);
            }
            (state.popup.as_ref().unwrap().clone(), state.parent.clone(), point(px(x), px(160.)))
        });
        let body = deferred(anchored().position(position).child(
            div().relative().child(popup).child(canvas(|_, _, _| (), move |bounds, _, _, _| measured.set(Some(bounds))).absolute().inset_0())
        )).with_priority(2);
        Some(SidebarPanel { title: "Submenu cascade scenario".into(), body: div().child(body).into_any_element() })
    }
    pub(super) fn run() {
        let _probe = buttons::Probe::new();
        for case in 0..3 {
            let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
            s.size(900., 600.);
            let _reset = Reset(FORCED_SIDEBAR.with(|slot| slot.get()));
            let parent = Rc::new(Cell::new(None));
            let invoked = Rc::new(Cell::new(0));
            let dismissed = Rc::new(Cell::new(0));
            STATE.with(|slot| *slot.borrow_mut() = Some(State { case, popup: None, parent: parent.clone(), invoked: invoked.clone(), dismissed: dismissed.clone(), subscription: None }));
            buttons::clear();
            FORCED_SIDEBAR.with(|slot| slot.set(Some((panel, "submenu-cascade-review", crate::shell::panel_dialog::PanelMode::Sidebar))));
            s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
            s.settle();
            let bounds = parent.get().expect("parent popup is rendered");
            let center = (f32::from(bounds.center().x), f32::from(bounds.center().y));
            s.hover(center);
            if case == 2 {
                assert!(buttons::bounds("cascade-review-leaf").is_none(), "both-side overflow must not open on hover");
                s.click(center);
            }
            let leaf = buttons::bounds("cascade-review-leaf").expect("submenu leaf is rendered");
            assert!(leaf.left() >= px(0.) && leaf.right() <= px(900.) && leaf.top() >= px(0.) && leaf.bottom() <= px(600.), "submenu fits viewport: {leaf:?}");
            match case {
                0 => assert!(leaf.left() >= bounds.right(), "right submenu must leave its parent visible: {bounds:?} / {leaf:?}"),
                1 => assert!(leaf.right() <= bounds.left(), "left submenu must leave its parent visible: {bounds:?} / {leaf:?}"),
                _ => {},
            }
            buttons::click(&mut s, "cascade-review-leaf");
            assert_eq!(invoked.get(), 1, "real leaf click invokes the handler");
            assert_eq!(dismissed.get(), 1, "real leaf click dismisses the parent chain");
            if case == 2 {
                buttons::clear();
                s.hover(center);
                assert!(buttons::bounds("cascade-review-leaf").is_none());
                s.update(|_, window, cx| STATE.with(|slot| slot.borrow().as_ref().unwrap().popup.as_ref().unwrap().focus_handle(cx).focus(window, cx)));
                s.key("enter");
                assert!(buttons::bounds("cascade-review-leaf").is_some(), "Enter explicitly opens a narrow submenu");
                s.key("enter");
                assert_eq!(invoked.get(), 2, "keyboard confirm invokes the child handler");
                assert_eq!(dismissed.get(), 2, "keyboard confirm preserves the dismiss chain");
            }
        }
    }
}
