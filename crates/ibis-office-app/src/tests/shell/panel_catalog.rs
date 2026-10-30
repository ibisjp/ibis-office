// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Opt-in gallery restored from the 2026-10-22 screenshot scenario.
use crate::shell::scenario::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::panel_dialog::PanelMode;
pub(crate) fn dump_all_right_panels() {
    use crate::shell::chrome::{FORCED_SIDEBAR, LAST_SIDEBAR_TITLE, SidebarPanel};
    use crate::shell::editor::SlideEditor;
    use gpui_kit::{Context, Window};

    let root = crate::shell::scenario::repo_root().join("target/right-panels-current");
    for sub in ["docx", "xlsx", "pptx"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    let manifest_path = root.join("manifest.tsv");
    let mut existing: std::collections::BTreeMap<(String, String), String> = std::collections::BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(&manifest_path) {
        for line in text.lines().filter(|line| !line.contains("\tmerge_manual_editor\t")) {
            let mut parts = line.splitn(4, '\t');
            if let (Some(fmt), Some(idx_s), Some(name)) = (parts.next(), parts.next(), parts.next()) {
                if let Ok(idx) = idx_s.parse::<usize>() {
                    existing.insert((fmt.to_string(), name.to_string()), line.to_string());
                }
            }
        }
    }

    let requested=std::env::var("IBIS_GALLERY_ONLY").unwrap_or_default();
    if requested.is_empty(){existing.clear();}
    let requested_formats:std::collections::BTreeSet<String>=existing.values().filter_map(|line|{
        let mut parts=line.split('\t');let fmt=parts.next()?;let _=parts.next()?;let name=parts.next()?;
        requested.split(',').any(|key|key==name).then(||fmt.to_string())
    }).collect();
    let wants_format=|fmt:&str|requested.is_empty()||requested_formats.is_empty()||requested_formats.contains(fmt);
    let mut snap = |s: &mut Scenario, fmt: &str, idx: usize, name: &'static str, f: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> Option<SidebarPanel>| {
        if let Ok(only)=std::env::var("IBIS_GALLERY_ONLY") {
            if !only.split(',').any(|entry|entry==name) {s.update(|e,w,cx|e.close_panel_dialog(w,cx));return;}
        }
        let file_name = format!("{idx:02}-{name}.png");
        let panel_path = root.join(fmt).join(&file_name);
        let win_path = root.join("window").join(fmt).join(&file_name);
        super::button_probe::clear();
        let (key, mode) = panel_mode(fmt, name);
        let capture_mode=if name.starts_with("tab_row_editor") {PanelMode::Modal} else {mode};
        FORCED_SIDEBAR.with(|c| c.set(Some((f, key, mode))));
        LAST_SIDEBAR_TITLE.with(|t| t.borrow_mut().clear());
        s.update(|e, _, cx| {
            e.sidebar_open = true;
            e.sidebar_scroll.set_offset(gpui_kit::Point::default());
            cx.notify();
        });
        let title = LAST_SIDEBAR_TITLE.with(|t| t.borrow().clone());
        assert!(!title.is_empty(), "{fmt}::{name} returned None (no sidebar title rendered)");
        if super::dialog_apply::active(){
            if mode==PanelMode::Modal{super::dialog_apply::audit(s,fmt,name);}
            FORCED_SIDEBAR.with(|c|c.set(None));
            s.update(|e,w,cx|e.close_panel_dialog(w,cx));return;
        }
        let (w, raw) = s.picture();
        let h = raw.len() as u32 / (w * 4);
        let img = image::RgbaImage::from_raw(w, h, raw).expect("rgba image");
        // The gallery publishes body crops only; do not retain whole-window copies.
        // Keep the normal window image, but show every settings section in the body image.
        let full_content=matches!(name,"sparkline_panel"|"conditional_panel"|"custom_format_panel"|"text_columns_panel"|"print_panel"|"threed_model_panel"|"import_text_panel"|"animation_pane");
        let (w,img)=if full_content {
            s.size(1200.,2000.);
            s.update(|e,_,cx|{e.panel_dialog.bounds=None;cx.notify();});
            let (width,rgba)=s.picture();let height=rgba.len() as u32/(width*4);
            (width,image::RgbaImage::from_raw(width,height,rgba).unwrap())
        }else{(w,img)};
        let h=img.height();
        let scale = w / 1200;
        let (sx, sy, sw, sh) = s.with(|e, _| {
            if e.panel_dialog.active {
                let b = e.panel_dialog.bounds.expect("dialog bounds");
                ((f32::from(b.origin.x) * scale as f32).round() as u32,
                 (f32::from(b.origin.y) * scale as f32).round() as u32,
                 (f32::from(b.size.width) * scale as f32).round() as u32,
                 (f32::from(b.size.height) * scale as f32).round() as u32)
            } else { (w.saturating_sub(300 * scale), 108 * scale, 300 * scale, h.saturating_sub((108 + 28) * scale)) }
        });
        let cropped = image::imageops::crop_imm(&img, sx, sy, sw, sh).to_image();
        cropped.save(&panel_path).unwrap();
        if full_content {s.size(1200.,720.);s.update(|e,_,cx|{e.panel_dialog.bounds=None;cx.notify();});}
        println!("DUMPED {fmt} #{idx:02} {name} -> title={title:?}");
        existing.retain(|(format,_),line|format!=fmt || line.split('\t').nth(2)!=Some(name));
        existing.insert((fmt.to_string(), name.to_string()), format!("{fmt}\t{idx:02}\t{name}\t{title}\t{}\t{}\t{capture_mode:?}", panel_path.display(), win_path.display()));
        let lines: Vec<String> = existing.values().cloned().collect();
        std::fs::write(&manifest_path, lines.join("\n") + "\n").unwrap();
        FORCED_SIDEBAR.with(|c| c.set(None));
        s.update(|e, window, cx| e.close_panel_dialog(window, cx));
    };

    // ==================== 1. DOCX ====================
    if wants_format("docx") {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let first_para = s.with(|e, _| {
            e.flow().unwrap().body.iter().find_map(|b| match b {
                ibis_doc_model::Block::Paragraph(p) => Some(p.id),
                _ => None,
            }).unwrap()
        });
        s.double_click_text(ibis_doc_model::FlowPos::new(first_para, 0));
        let mut i = 0usize;
        macro_rules! d {
            ($name:expr, $setup:expr, $panel:ident) => {{
                i += 1;
                s.update($setup);
                snap(&mut s, "docx", i, $name, |e, _, cx| e.$panel(cx));
            }};
        }
        d!("font_panel", |e, w, cx| e.open_font_edit(w, cx), font_panel);
        d!("color_panel", |e, w, cx| e.open_color_edit(false, w, cx), color_panel);
        d!("docx_compress_panel", |e, w, cx| e.open_docx_compress_panel(w, cx), docx_compress_panel);
        d!("paragraph_panel", |e, w, cx| e.open_paragraph_edit(None, w, cx), paragraph_panel);
        d!("tab_panel", |e, w, cx| {
            e.open_tab_edit(w,cx);e.clear_all_tab_stops(w,cx);e.apply_tab_edit(w,cx);
            for value in ["72","144","216"] {let input=e.doc_state().unwrap().tab_edit.as_ref().unwrap().pos_field.clone();input.update(cx,|i,cx|i.set_value(value,w,cx));e.set_tab_stop(w,cx);e.apply_tab_edit(w,cx);}
            e.select_tab_stop(1,w,cx);
        }, tab_panel);
        d!("comments_panel", |e, w, cx| {e.set_caret(ibis_doc_model::FlowPos::new(first_para,0),false,cx);e.set_caret(ibis_doc_model::FlowPos::new(first_para,1),true,cx);e.open_comments(w,cx);e.send_comment("Please check this section.",w,cx);let id=e.flow().unwrap().comments.last().unwrap().id;e.reply_to(id,w,cx);e.send_comment("Checked. Thank you.",w,cx);}, comments_panel);
        d!("style_panel", |e, w, cx| e.open_style_edit(w, cx), style_panel);
        d!("link_panel", |e, w, cx| e.open_link(w, cx), link_panel);
        d!("find_panel", |e, w, cx| {e.open_find(w,cx);e.doc_state_mut().unwrap().find.as_mut().unwrap().replace_mode=true;}, find_panel);
        d!("ruby_panel", |e,w,cx|{
            let (para,offset)=e.flow().unwrap().body.iter().find_map(|b| {
                if let ibis_doc_model::Block::Paragraph(p)=b {let mut at=0;for run in &p.runs {if matches!(&run.kind,ibis_doc_model::FlowRunKind::Text(t) if !t.is_empty()){return Some((p.id,at));}at+=run.len();}}None
            }).expect("plain text for ruby");
            let state=e.doc_state_mut().unwrap();state.anchor=Some(ibis_doc_model::FlowPos::new(para,offset));state.caret=Some(ibis_doc_model::FlowPos::new(para,offset+1));
            e.open_ruby(w,cx);assert!(e.doc_state().unwrap().ruby.is_some(),"ruby fixture selected plain text");
        },ruby_panel);
        d!("merge_panel", |e,w,cx|{e.open_merge(w,cx);let input=e.doc_state().unwrap().merge.as_ref().unwrap().text.clone();input.update(cx,|i,cx|i.set_value("Name,Company,City\nAlice,Sample A,Tokyo\nBob,Sample B,Osaka\nCarol,Sample C,Nagoya",w,cx));e.apply_merge_data(cx);},merge_panel);
        d!("doc_page_setup_panel", |e, w, cx| e.open_page_setup_panel(w, cx), doc_page_setup_panel);
        d!("notes_panel", |e, w, cx| e.open_notes_panel(w, cx), notes_panel);
        d!("bookmark_panel", |e, w, cx| e.open_bookmark_panel(w, cx), bookmark_panel);
        d!("watermark_panel", |e, w, cx| e.open_watermark_panel(w, cx), watermark_panel);
        d!("new_style_panel", |e, w, cx| e.open_new_style_panel(w, cx), new_style_panel);
        d!("symbol_panel", |e, _, cx| e.open_symbols(cx), symbol_panel);
        d!("word_count_panel", |e, w, cx| e.open_word_count_panel(w, cx), word_count_panel);
        d!("field_panel", |e,w,cx|{e.open_field_panel(w,cx);let code=e.doc_state().unwrap().helper.field.as_ref().unwrap().code.clone();code.update(cx,|i,cx|i.set_value("PAGE",w,cx));},field_panel);
        d!("caption_panel", |e, w, cx| e.open_caption_panel(w, cx), caption_panel);
        d!("doc_properties_panel", |e, w, cx| e.open_doc_properties_panel(w, cx), doc_properties_panel);
        d!("cross_reference_panel", |e, w, cx| e.open_cross_reference_panel(w, cx), cross_reference_panel);
        d!("selection_pane_panel", |e, _, cx| e.open_selection_pane(cx), selection_pane_panel);
        d!("accessibility_panel", |e, _, cx| e.open_accessibility_panel(cx), accessibility_panel);
        d!("update_toc_panel", |e, _, cx| e.open_update_toc_panel(cx), update_toc_panel);
        d!("user_info_panel", |e, w, cx| e.open_user_info_panel(w, cx), user_info_panel);
        d!("kinsoku_panel", |e, w, cx| e.open_kinsoku_panel(w, cx), kinsoku_panel);
        d!("go_to_panel", |e, w, cx| e.open_go_to_panel(w, cx), go_to_panel);
        d!("doc_spelling_panel", |e, w, cx| e.open_doc_spelling(w, cx), doc_spelling_panel);
        d!("doc_autocorrect_panel", |e, w, cx| e.open_doc_autocorrect_panel(w, cx), doc_autocorrect_panel);
        d!("autorecover_settings_panel", |e, w, cx| e.open_autorecover_settings(w, cx), autorecover_settings_panel);
        d!("recover_unsaved_panel", |e, _, cx| {e.open_recover_unsaved(cx);seed_recovery_gallery(e);}, recover_unsaved_panel);
        d!("docx_math_panel", |e,w,cx| {e.open_docx_math_panel(w,cx);let input=e.doc_state().unwrap().docx_math.as_ref().unwrap().input.clone();input.update(cx,|i,cx|i.set_value("x^2/2 + √(a+b)",w,cx));}, docx_math_panel);
        d!("quick_parts_panel", |e, w, cx| e.open_quick_parts_panel(w, cx), quick_parts_panel);
        d!("doc_inspect_panel", |e, w, cx| e.open_doc_inspect_panel(w, cx), doc_inspect_panel);
        d!("doc_compatibility_panel", |e, w, cx| e.open_doc_compatibility_panel(w, cx), doc_compatibility_panel);
        d!("doc_print_panel", |e, w, cx| e.open_doc_print_panel(w, cx), doc_print_panel);
        d!("doc_source_manager_panel", |e, w, cx| e.open_source_manager_panel(w, cx), doc_source_manager_panel);
        d!("doc_thesaurus_panel", |e, w, cx| e.open_thesaurus_panel(w, cx), doc_thesaurus_panel);
        d!("doc_repair_panel", |e, w, cx| e.open_docx_repair_panel(w, cx), doc_repair_panel);
        d!("doc_template_panel", |e, w, cx| e.open_docx_template_panel(w, cx), doc_template_panel);
        d!("list_style_panel", |e, w, cx| e.open_define_list_style_panel(w, cx), list_style_panel);
        d!("docx_theme_panel", |e, w, cx| e.open_docx_theme_panel(w, cx), docx_theme_panel);
        d!("theme_palette_editor", |e,w,cx| {e.open_docx_theme_panel(w,cx);e.theme_colors_ui.editing=true;}, docx_theme_panel);
        d!("master_doc_panel", |e, w, cx| e.open_master_doc_panel(w, cx), master_doc_panel);
        d!("docx_ink_panel", |e, _, cx| e.open_docx_ink_panel(cx), docx_ink_panel);
        d!("docx_screen_reader_panel", |e, _, cx| e.open_docx_screen_reader_panel(cx), docx_screen_reader_panel);
        d!("docx_read_aloud_panel", |e, _, cx| e.open_docx_read_aloud(cx), docx_read_aloud_panel);
        d!("docx_customize_toolbar_panel", |e, _, cx| e.open_docx_customize_toolbar_panel(cx), docx_customize_toolbar_panel);

        // Picture panel: select the picture in docx-report.docx
        let pic_pos = s.with(|e, _| {
            let doc = e.flow().unwrap();
            for b in &doc.body {
                if let ibis_doc_model::Block::Paragraph(p) = b {
                    let mut off = 0;
                    for r in &p.runs {
                        if matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. }) {
                            return ibis_doc_model::FlowPos::new(p.id, off);
                        }
                        off += r.len();
                    }
                }
            }
            panic!("picture in docx-report.docx");
        });
        d!("picture_panel", move |e, w, cx| {
            e.set_caret(pic_pos, false, cx);
            e.open_picture_edit(w, cx);
        }, picture_panel);

        // Table panels: place caret in the table in docx-report.docx
        let tbl_pos = s.with(|e, _| {
            let doc = e.flow().unwrap();
            for b in &doc.body {
                if let ibis_doc_model::Block::Table(t) = b {
                    for b2 in &t.rows[0].cells[0].blocks {
                        if let ibis_doc_model::Block::Paragraph(p) = b2 {
                            return ibis_doc_model::FlowPos::new(p.id, 0);
                        }
                    }
                }
            }
            panic!("table in docx-report.docx");
        });
        d!("table_size_panel", move |e, w, cx| {
            e.set_caret(tbl_pos, false, cx);
            e.open_table_size(w, cx);
        }, table_size_panel);
        d!("table_formula_panel", move |e, w, cx| {
            e.set_caret(tbl_pos, false, cx);
            e.open_table_formula(w, cx);
        }, table_formula_panel);

        // Text box panel: insert a text box at first paragraph
        d!("text_box_panel", move |e, w, cx| {
            e.set_caret(ibis_doc_model::FlowPos::new(first_para, 0), false, cx);
            e.doc_insert_text_box(w, cx);
            e.open_text_box(w, cx);
        }, text_box_panel);

        // Chart panel: insert a chart
        d!("doc_chart_panel", move |e, w, cx| {
            e.set_caret(ibis_doc_model::FlowPos::new(first_para, 0), false, cx);
            e.doc_insert_chart(ibis_pptx::NewChartKind::Column3D, w, cx);
            e.open_doc_chart_panel(w, cx);
        }, doc_chart_panel);

        // Content control panel: insert a picture control and select it
        s.update(move |e, _, cx| e.set_caret(ibis_doc_model::FlowPos::new(first_para, 0), false, cx));
        s.menu("insert.picture-control");
        d!("control_panel", move |e, w, cx| {
            e.set_caret(ibis_doc_model::FlowPos::new(first_para, 0), false, cx);
            e.open_control_properties(w, cx);
        }, control_panel);
        d!("ruler_panel", |e, _, cx| e.open_ruler_panel(cx), ruler_panel);
        d!("tab_row_editor", |e,w,cx| {e.ruler_settings_open=false;e.open_tab_edit(w,cx);e.open_tab_row_editor(None,w,cx);}, tab_panel);
        d!("tab_row_editor_edit", |e,w,cx|{e.open_tab_edit(w,cx);for value in ["72","144","216"]{let i=e.doc_state().unwrap().tab_edit.as_ref().unwrap().pos_field.clone();i.update(cx,|i,cx|i.set_value(value,w,cx));e.set_tab_stop(w,cx);e.apply_tab_edit(w,cx);}e.open_tab_row_editor(Some(1),w,cx);},tab_panel);
        d!("merge_row_editor", |e,w,cx| {e.open_merge(w,cx);let input=e.doc_state().unwrap().merge.as_ref().unwrap().text.clone();input.update(cx,|i,cx|i.set_value("Name,City\nAlice,Tokyo\nBob,Osaka",w,cx));e.apply_merge_data(cx);e.open_merge_row(Some(0),w,cx);}, merge_row_dialog);
        s.update(|e,w,cx| {e.open_doc_autocorrect_panel(w,cx);seed_autocorrect_entry(e,w,cx);});
        i+=1;
        snap(&mut s,"docx",i,"autocorrect_entry",|e,_,cx|e.doc_autocorrect_panel(cx));
    }

    // ==================== 2. XLSX ====================
    if wants_format("xlsx") {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let mut i = 0usize;
        macro_rules! x {
            ($name:expr, $setup:expr, $panel:ident) => {{
                i += 1;
                s.update($setup);
                snap(&mut s, "xlsx", i, $name, |e, _, cx| e.$panel(cx));
            }};
        }
        x!("symbol_panel", |e, _, cx| e.open_symbols(cx), symbol_panel);
        x!("custom_format_panel", |e, w, cx| {
            e.open_custom_format(w, cx);
            e.wb_mut().format_open = [true; 6];
        }, custom_format_panel);
        x!("page_setup_panel", |e, _, cx| {
            if let Some(st) = e.sheet_state_mut() { st.header_footer_edit = None; }
            e.open_page_setup(cx);
        }, page_setup_panel);
        x!("header_footer_edit_panel", |e, w, cx| {
            e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Header, w, cx);
        }, header_footer_edit_panel);
        x!("cell_link_panel", |e, w, cx| e.open_link_panel(w, cx), cell_link_panel);
        x!("sheet_find_panel", |e, w, cx| { e.open_sheet_find(w, cx); e.sheet_state_mut().unwrap().find.as_mut().unwrap().replace_mode=true; }, sheet_find_panel);
        x!("sheet_spelling_panel", |e, w, cx| e.open_sheet_spelling(w, cx), sheet_spelling_panel);
        x!("validation_panel", |e, w, cx| e.open_validation_panel(w, cx), validation_panel);
        x!("names_panel", |e, w, cx| e.open_names(w, cx), names_panel);
        x!("conditional_panel", |e, w, cx| e.open_conditional(None, w, cx), conditional_panel);
        x!("sort_panel", |e, w, cx| e.open_sort_panel(w, cx), sort_panel);
        x!("dedupe_panel", |e, w, cx| e.open_dedupe_panel(w, cx), dedupe_panel);
        x!("recommended_charts_panel", |e, w, cx| e.open_recommended_charts(w, cx), recommended_charts_panel);
        x!("text_columns_panel", |e, w, cx| e.open_text_columns_panel(w, cx), text_columns_panel);
        x!("import_text_panel", |e,w,cx| {e.show_import_text_panel(w,cx);e.set_import_text_data("sample.csv".into(),b"Name,Company,Amount\nAlice,North,1200\nBob,South,1800\nCarol,East,1500\nDave,West,2100\nEve,Central,900\n".to_vec(),cx);let input=e.wb().panels.import_text.as_ref().unwrap().file_path.clone();input.update(cx,|i,cx|i.set_value("sample.csv",w,cx));}, import_text_panel);
        x!("queries_panel", |e, _, cx| e.open_queries_panel(cx), queries_panel);
        x!("cells_dialog_panel_insert", |e, _, cx| e.open_cells_dialog(true, cx), cells_dialog_panel);
        x!("cells_dialog_panel_delete", |e, _, cx| e.open_cells_dialog(false, cx), cells_dialog_panel);
        x!("external_links_panel", |e, w, cx| e.open_external_links_panel(w, cx), external_links_panel);
        x!("goto_special_panel", |e, _, cx| e.open_goto_special_panel(cx), goto_special_panel);
        x!("note_panel", |e, w, cx| e.open_note_panel(w, cx), note_panel);
        x!("threaded_comment_panel", |e, w, cx| e.open_threaded_comment_panel(w, cx), threaded_comment_panel);
        x!("protect_panel", |e, w, cx| e.open_protect_panel(w, cx), protect_panel);
        x!("protect_workbook_panel", |e, w, cx| e.open_protect_workbook_panel(w, cx), protect_workbook_panel);
        x!("allow_edit_ranges_panel", |e, w, cx| e.open_allow_edit_ranges_panel(w, cx), allow_edit_ranges_panel);
        x!("series_panel", |e, w, cx| e.open_series_panel(w, cx), series_panel);
        x!("goal_seek_panel", |e, w, cx| e.open_goal_seek(w, cx), goal_seek_panel);
        x!("function_panel", |e, w, cx| e.open_function_panel(w, cx), function_panel);
        x!("sheet_word_count_panel", |e, w, cx| e.open_sheet_word_count(w, cx), sheet_word_count_panel);
        x!("sheet_thesaurus_panel", |e, w, cx| e.open_sheet_thesaurus(w, cx), sheet_thesaurus_panel);
        x!("data_table_panel", |e, w, cx| {
            let slide = e.current_slide;
            let state = e.wb_mut();
            state.cells.insert(slide, (0, 1));
            state.cell_ends.insert(slide, (2, 3));
            e.open_data_table(w, cx);
        }, data_table_panel);
        x!("scenario_manager_panel", |e, w, cx| e.open_scenario_manager(w, cx), scenario_manager_panel);
        x!("sparkline_panel", |e, w, cx| e.open_sparkline_panel(None, w, cx), sparkline_panel);
        x!("subtotal_panel", |e, w, cx| e.open_subtotal_panel(w, cx), subtotal_panel);
        x!("pivot_panel_create", |e, w, cx| e.open_create_pivot(w, cx), pivot_panel);
        x!("advanced_filter_panel", |e, w, cx| e.open_advanced_filter_panel(w, cx), advanced_filter_panel);
        x!("consolidate_panel", |e, w, cx| e.open_consolidate_panel(w, cx), consolidate_panel);
        x!("analysis_panel", |e, w, cx| e.open_analysis_panel(w, cx), analysis_panel);
        x!("solver_panel", |e, w, cx| {e.open_solver_panel(w,cx);let p=e.wb_mut().panels.solver.as_mut().unwrap();p.constraints=vec![("B2".into(),ibis_doc_model::solver::Rel::Le,"100".into()),("B3".into(),ibis_doc_model::solver::Rel::Ge,"0".into()),("B4".into(),ibis_doc_model::solver::Rel::Int,"".into())];}, solver_panel);
        x!("solver_constraint_editor", |e,w,cx| {e.open_solver_panel(w,cx);e.open_solver_constraint(None,w,cx);}, solver_panel);
        x!("ink_panel", |e, w, cx| e.open_ink_panel(w, cx), ink_panel);
        x!("slicer_insert_panel", |e, w, cx| e.open_slicer_insert_panel(w, cx), slicer_insert_panel);
        x!("timeline_insert_panel", |e, w, cx| e.open_timeline_insert_panel(w, cx), timeline_insert_panel);
        x!("error_options_panel", |e, w, cx| e.open_error_options_panel(w, cx), error_options_panel);
        x!("autocorrect_panel", |e, w, cx| e.open_autocorrect_panel(w, cx), autocorrect_panel);
        x!("forecast_panel", |e, w, cx| e.open_forecast_panel(w, cx), forecast_panel);
        x!("new_cell_style_panel", |e, w, cx| e.open_new_cell_style_panel(w, cx), new_cell_style_panel);
        x!("xml_source_panel", |e, w, cx| e.open_xml_source(w, cx), xml_source_panel);
        x!("xml_map_properties_panel", |e, w, cx| e.open_xml_map_properties(w, cx), xml_map_properties_panel);
        x!("evaluate_formula_panel", |e, w, cx| e.open_evaluate_formula(w, cx), evaluate_formula_panel);
        x!("watch_window_panel", |e,w,cx| {e.open_watch_window(w,cx);seed_watch_gallery(e);}, watch_window_panel);
        x!("watch_editor", |e,w,cx| {e.open_watch_window(w,cx);seed_watch_gallery(e);e.edit_watch(1,w,cx);}, watch_window_panel);
        x!("sheet_doc_properties_panel", |e, w, cx| e.open_sheet_doc_properties_panel(w, cx), sheet_doc_properties_panel);
        x!("xlsx_accessibility_panel", |e, w, cx| e.open_xlsx_accessibility_panel(w, cx), xlsx_accessibility_panel);
        x!("xlsx_compatibility_panel", |e, w, cx| e.open_xlsx_compatibility_panel(w, cx), xlsx_compatibility_panel);
        x!("xlsx_selection_pane_panel", |e,w,cx| {for _ in 0..3 {e.insert_sheet_shape("rect",cx);} e.open_xlsx_selection_pane(w,cx);}, xlsx_selection_pane_panel);
        x!("xlsx_theme_panel", |e, w, cx| e.open_xlsx_theme_panel(w, cx), xlsx_theme_panel);
        x!("theme_palette_editor", |e,w,cx| {e.open_xlsx_theme_panel(w,cx);e.theme_colors_ui.editing=true;}, xlsx_theme_panel);
        x!("custom_views_panel", |e, w, cx| e.open_custom_views_panel(w, cx), custom_views_panel);
        x!("unhide_window_panel", |e, _, cx| e.open_unhide_window_panel(cx), unhide_window_panel);
        x!("arrange_windows_panel", |e, _, cx| e.open_arrange_windows_panel(cx), arrange_windows_panel);
        x!("xlsx_backup_save_settings_panel", |e, w, cx| e.open_xlsx_backup_save_settings_panel(w, cx), xlsx_backup_save_settings_panel);
        x!("autorecover_settings_panel", |e, w, cx| e.open_autorecover_settings(w, cx), autorecover_settings_panel);
        x!("recover_unsaved_panel", |e, _, cx| {e.open_recover_unsaved(cx);seed_recovery_gallery(e);}, recover_unsaved_panel);
        x!("xlsx_repair_panel", |e, w, cx| e.open_xlsx_repair_panel(w, cx), xlsx_repair_panel);
        x!("phonetic_panel", |e, w, cx| e.open_phonetic_panel(w, cx), phonetic_panel);
        x!("row_col_size_panel_row", |e, w, cx| e.open_row_height_panel(w, cx), row_col_size_panel);
        x!("row_col_size_panel_col", |e, w, cx| e.open_column_width_panel(w, cx), row_col_size_panel);

        // Paste special panel: copy a cell first
        s.click_cell("B3");
        s.key("cmd-c");
        x!("paste_special_panel", |e, _, cx| e.open_paste_special_dialog(cx), paste_special_panel);

        // Table design & new table style panels: create a table on A1:D5
        s.click_cell("A2");
        x!("table_design_panel", |e, w, cx| {
            let _ = e.make_table(ibis_doc_model::CellRange { first_column: 0, first_row: 1, last_column: 3, last_row: 5 }, true, cx);
            e.open_table_design(w, cx);
        }, table_design_panel);
        x!("new_table_style_panel", |e, w, cx| e.open_new_table_style(w, cx), new_table_style_panel);

        // Chart panels: select the chart on slide 0 of xlsx-sales.xlsx
        let chart_id = s.with(|e, _| {
            e.slide().shapes.iter().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap().id
        });
        s.update(move |e, _, _| e.selection = Some(chart_id));
        x!("sheet_chart_panel", |e, w, cx| e.open_sheet_chart_panel(w, cx), sheet_chart_panel);
        x!("move_chart_panel", |e, w, cx| e.open_move_chart_panel(w, cx), move_chart_panel);
        x!("chart_template_panel", |e, w, cx| e.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, w, cx), chart_template_panel);
        x!("sheet_alt_text_panel", |e, w, cx| e.open_sheet_alt_text_panel(w, cx), sheet_alt_text_panel);

        // Shape frame panel & control properties panel
        s.menu("xlsx.insert-textbox");
        s.key("escape");
        x!("sheet_frame_panel", |e, w, cx| {
            let id = e.slide().shapes.last().unwrap().id;
            e.selection = Some(id);
            e.open_sheet_frame_panel(w, cx);
        }, sheet_frame_panel);
        x!("control_properties_panel", |e, w, cx| {
            e.insert_form_control(ibis_doc_model::ControlKind::Button, cx);
            let id = e.slide().shapes.last().unwrap().id;
            e.selection = Some(id);
            e.open_control_properties_panel(w, cx);
        }, control_properties_panel);

        // Pivot & Slicer panels on xlsx-slicers-excel.xlsx
        let mut i_piv = i;
        drop(s);
        let mut sp = Scenario::open("test/editing/xlsx-slicers-excel.xlsx");
        let piv_cell = sp.with(|e, _| {
            let (_, sh) = e.current_sheet().unwrap();
            let p = &sh.pivots[0];
            (p.location.first_column, p.location.first_row + 1)
        });
        let piv_pt = sp.with(move |e, _| e.cell_center(piv_cell.0, piv_cell.1)).unwrap();
        sp.click((f32::from(piv_pt.x), f32::from(piv_pt.y)));
        macro_rules! xp {
            ($name:expr, $setup:expr, $panel:ident) => {{
                i_piv += 1;
                sp.update($setup);
                snap(&mut sp, "xlsx", i_piv, $name, |e, _, cx| e.$panel(cx));
            }};
        }
        xp!("pivot_panel_fields", |e, w, cx| e.open_pivot_fields(w, cx), pivot_panel);
        xp!("pivot_panel_value_settings", |e, w, cx| e.open_pivot_value_settings(w, cx), pivot_panel);
        xp!("pivot_panel_change_source", |e, w, cx| e.open_pivot_change_source(w, cx), pivot_panel);
        xp!("pivot_panel_move", |e, w, cx| e.open_pivot_move(w, cx), pivot_panel);
        xp!("pivot_group_panel", |e, w, cx| {
            let slide = e.current_slide;
            let (_, sh) = e.current_sheet().unwrap();
            let p = &sh.pivots[0];
            let cell = (p.location.first_column + p.first_data_col, p.location.first_row + p.first_data_row);
            let st = e.wb_mut();
            st.cells.insert(slide, cell);
            st.cell_ends.insert(slide, cell);
            e.open_pivot_group(w, cx);
        }, pivot_group_panel);
        xp!("pivot_options_panel", |e, w, cx| e.open_pivot_options(w, cx), pivot_options_panel);
        xp!("pivot_calc_field_panel", |e, w, cx| e.open_pivot_calc_field(w, cx), pivot_calc_field_panel);
        xp!("pivot_calc_item_panel", |e, w, cx| e.open_pivot_calc_item(w, cx), pivot_calc_item_panel);

        let slicer_id = sp.with(|e, _| {
            e.slide().shapes.iter().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Slicer(_))).unwrap().id
        });
        sp.update(move |e, _, _| e.selection = Some(slicer_id));
        xp!("slicer_settings_panel", |e, w, cx| e.open_slicer_settings_panel(w, cx), slicer_settings_panel);
        sp.update(|e,w,cx| {e.open_autocorrect_panel(w,cx);seed_autocorrect_entry(e,w,cx);});
        i+=1;
        snap(&mut sp,"xlsx",i,"autocorrect_entry",|e,_,cx|e.autocorrect_panel(cx));
    }

    // ==================== 3. PPTX (50 panels + chrome::sidebar + multi-mode) ====================
    if wants_format("pptx") {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        let mut i = 0usize;
        macro_rules! p {
            ($name:expr, $setup:expr, $panel:ident) => {{
                i += 1;
                s.update($setup);
                snap(&mut s, "pptx", i, $name, |e, _, cx| e.$panel(cx));
            }};
        }
        // Default chrome::sidebar without selection (Slide) and with selection (Format Options)
        i += 1;
        s.update(|e, _, _| e.selection = None);
        snap(&mut s, "pptx", i, "sidebar_slide", |e, w, cx| crate::pptx::chrome::sidebar(e,w,cx));
        s.click_shape(0);
        i += 1;
        snap(&mut s, "pptx", i, "sidebar_format_options", |e, w, cx| crate::pptx::chrome::sidebar(e,w,cx));

        p!("threed_model_panel", |e, w, cx| e.open_3d_model_panel(w,cx), threed_model_panel);
        p!("record_preview_panel", |e, _, cx| e.open_record_preview_panel(cx), record_preview_panel);
        p!("pptx_screen_reader_panel", |e, _, cx| e.open_pptx_screen_reader_panel(cx), pptx_screen_reader_panel);
        p!("pptx_ribbon_qat_panel", |e, _, cx| e.open_pptx_ribbon_qat_panel(cx), pptx_ribbon_qat_panel);
        p!("pptx_trust_center_panel", |e, _, cx| e.open_pptx_trust_center_panel(cx), pptx_trust_center_panel);
        p!("audio_record_panel", |e, w, cx| e.open_audio_record_panel(w, cx), audio_record_panel);
        p!("photo_album_panel", |e, w, cx| e.open_photo_album_panel(w, cx), photo_album_panel);
        p!("pptx_ink_panel", |e, w, cx| e.open_pptx_ink_panel(w, cx), pptx_ink_panel);
        p!("pptx_autocorrect_panel", |e, w, cx| e.open_pptx_autocorrect_panel(w, cx), pptx_autocorrect_panel);
        p!("pptx_math_panel", |e, w, cx| e.open_pptx_math_panel(w, cx), pptx_math_panel);
        p!("autorecover_settings_panel", |e, w, cx| e.open_autorecover_settings(w, cx), autorecover_settings_panel);
        p!("recover_unsaved_panel", |e, _, cx| {e.open_recover_unsaved(cx);seed_recovery_gallery(e);}, recover_unsaved_panel);
        p!("pptx_compatibility_panel", |e, w, cx| e.open_pptx_compatibility_panel(w, cx), pptx_compatibility_panel);
        p!("pptx_accessibility_panel", |e, w, cx| e.open_pptx_accessibility_panel(w, cx), pptx_accessibility_panel);
        p!("presentation_inspect_panel", |e, w, cx| e.open_presentation_inspect_panel(w, cx), presentation_inspect_panel);
        p!("presentation_properties_panel", |e, w, cx| e.open_presentation_properties_panel(w, cx), presentation_properties_panel);
        p!("symbol_panel", |e, _, cx| e.open_symbols(cx), symbol_panel);
        p!("notes_marks_panel", |e, w, cx| e.open_notes_marks(w, cx), notes_marks_panel);
        p!("spelling_panel", |e, w, cx| e.open_spelling(w, cx), spelling_panel);
        p!("thesaurus_panel", |e, w, cx| e.open_pptx_thesaurus(w, cx), pptx_thesaurus_panel);
        p!("compress_panel", |e, w, cx| e.open_compress_panel(w, cx), compress_panel);
        p!("print_panel", |e, w, cx| e.open_print_panel(w, cx), print_panel);
        p!("layout_name_panel", |e, w, cx| e.open_layout_rename(w, cx), layout_name_panel);
        p!("pptx_section_panel", |e, w, cx| e.open_section_panel(false, w, cx), pptx_section_panel);
        p!("pptx_gradient_panel_linear", |e, w, cx| e.open_gradient_panel(false, w, cx), pptx_gradient_panel);
        p!("pptx_gradient_panel_path", |e, w, cx| e.open_path_gradient_panel(false, w, cx), pptx_gradient_panel);
        p!("pptx_gradient_panel_pattern", |e, w, cx| e.open_pattern_panel(false, w, cx), pptx_gradient_panel);
        p!("shadow_panel", |e, w, cx| e.open_shadow_panel(w, cx), shadow_panel);
        p!("glow_panel", |e, w, cx| e.open_glow_panel(w, cx), glow_panel);
        p!("pptx_paragraph_panel", |e, w, cx| e.open_paragraph_panel(w, cx), pptx_paragraph_panel);
        p!("timing_panel", |e, w, cx| e.open_timing_panel(w, cx), timing_panel);
        p!("show_panel", |e, w, cx| e.open_show_panel(w, cx), show_panel);
        p!("frame_panel", |e, w, cx| e.open_frame_panel(w, cx), frame_panel);
        p!("alt_text_panel", |e, w, cx| e.open_alt_text_panel(w, cx), alt_text_panel);
        p!("header_footer_panel", |e, w, cx| e.open_header_footer(w, cx), header_footer_panel);
        p!("animation_pane", |e, _, cx| {
            use ibis_doc_model::*;
            let ids=seed_pptx_gallery_shapes(e,cx);
            let looks=[AnimationLook::Fade,AnimationLook::Fly(AnimationSide::Left),AnimationLook::Zoom];
            let animations=ids.into_iter().zip(looks).enumerate().map(|(i,(id,look))|{
                let mut a=crate::pptx::animation_ui::effect(id,AnimationClass::Entrance,look);
                a.duration=500+i as u32*250;if i>0{a.start=AnimationStart::AfterPrevious;}a
            }).collect();
            let slide=e.current_slide;e.run(vec![Command::SetAnimations{slide,animations}],cx);
            if e.pptx().is_none_or(|st| !st.animation_pane) { e.toggle_animation_pane(cx); }
        }, animation_pane);
        s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
        p!("animation_options_panel", |e, _, cx| e.open_animation_options_panel(0, cx), animation_options_panel);
        p!("reading_order_pane", |e, _, cx| e.open_reading_order_pane(cx), reading_order_pane);
        p!("selection_pane", |e, _, cx| {
            seed_pptx_gallery_shapes(e,cx);
            if e.pptx().is_none_or(|st| st.selection_pane.is_none()) { e.toggle_selection_pane(cx); }
        }, selection_pane);
        p!("slide_link_panel", |e, w, cx| e.slide_open_link_panel(w, cx), slide_link_panel);
        p!("theme_panel", |e, w, cx| e.open_theme_panel(w, cx), theme_panel);
        p!("theme_palette_editor", |e,w,cx| {e.open_theme_panel(w,cx);e.theme_colors_ui.editing=true;}, theme_panel);
        p!("text_insets_panel", |e, w, cx| e.open_text_insets_panel(w, cx), text_insets_panel);
        p!("slide_size_panel", |e, w, cx| e.open_slide_size_panel(w, cx), slide_size_panel);
        p!("pptx_find_panel", |e, w, cx| { e.open_pptx_find(w, cx); e.pptx_mut().unwrap().find.as_mut().unwrap().replace_mode=true; }, pptx_find_panel);
        p!("replace_fonts_panel", |e, w, cx| e.open_replace_fonts_panel(w, cx), replace_fonts_panel);
        p!("slide_comments_panel", |e, w, cx| e.open_slide_comments(w, cx), slide_comments_panel);

        // SmartArt text pane
        p!("smartart_text_pane", |e, _, cx| {
            e.insert_smartart(crate::pptx::SmartArtKind::BasicBlockList, cx);
            if e.pptx().is_none_or(|st| !st.smartart_text_pane) { e.toggle_smartart_text_pane(cx); }
        }, smartart_text_pane);

        // Compare panel
        let other_pptx = crate::shell::scenario::repo_root().join("test/editing/pptx-plan.pptx");
        p!("compare_panel", move |e, _, cx| {
            e.compare_presentation_with_file(&other_pptx, cx);
        }, compare_panel);

        // Picture crop panel: find a slide with a picture
        let (pic_page, pic_idx) = s.with(|e, _| {
            e.presentation().slides.iter().enumerate().find_map(|(si, sl)| {
                sl.shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Picture(_))).map(| j | (si, j))
            }).unwrap()
        });
        s.page(pic_page + 1);
        s.click_shape(pic_idx);
        p!("crop_panel", |e, w, cx| e.open_crop_panel(w, cx), crop_panel);

        // Chart panel: insert a chart on slide 2 of pptx-plan.pptx
        s.page(2);
        s.menu("pptx.insert-chart.column");
        p!("chart_panel", |e, w, cx| e.open_chart_panel(w, cx), chart_panel);

        // Table cell size panel: insert a table and open cell_size_panel
        p!("cell_size_panel", |e, w, cx| {
            e.insert_slide_table(3, 3, w, cx);
            e.open_cell_size_panel(w, cx);
        }, cell_size_panel);

        // Master view & Notes format panels
        p!("notes_format_panel", |e, w, cx| e.open_notes_format(w, cx), notes_format_panel);
        p!("notes_master_panel", |e, w, cx| {e.close_notes_format(w,cx);e.open_notes_master(w, cx);}, notes_format_panel);
        p!("handout_master_panel", |e, w, cx| {e.close_notes_format(w,cx);e.open_handout_master(w, cx);}, notes_format_panel);
        s.update(|e, w, cx| e.close_notes_format(w, cx));
        p!("master_view_panel", |e, w, cx| e.open_master_view(w, cx), master_view_panel);
        s.update(|e,w,cx| {e.open_pptx_autocorrect_panel(w,cx);seed_autocorrect_entry(e,w,cx);});
        i+=1;
        snap(&mut s,"pptx",i,"autocorrect_entry",|e,_,cx|e.pptx_autocorrect_panel(cx));
    }
}
fn panel_mode(_format: &str, name: &'static str) -> (&'static str, PanelMode) {
    let key = match name {
        "row_col_size_panel_row" => "row_col_size_panel",
        "row_col_size_panel_col" => "row_col_size_panel",
        "cells_dialog_panel_insert" => "cells_dialog_panel",
        "cells_dialog_panel_delete" => "cells_dialog_panel",
        "pivot_panel_fields" => "pivot_panel",
        "pivot_panel_value_settings" => "pivot_panel",
        "pivot_panel_change_source" => "pivot_panel",
        "pivot_panel_move" => "pivot_panel",
        "pivot_panel_create" => "pivot_panel",
        "pptx_gradient_panel_linear" => "pptx_gradient_panel",
        "pptx_gradient_panel_path" => "pptx_gradient_panel",
        "pptx_gradient_panel_pattern" => "pptx_gradient_panel",
        _ => name,
    };
    let mode = match key {
        "merge_row_editor" | "names_panel" | "conditional_panel" => PanelMode::Modal,
        "sheet_alt_text_panel" | "theme_palette_editor" | "solver_constraint_editor" | "watch_editor" | "autocorrect_entry" | "advanced_filter_panel" | "allow_edit_ranges_panel" | "alt_text_panel" | "analysis_panel" | "animation_options_panel" | "arrange_windows_panel" | "audio_record_panel" | "autocorrect_panel" | "autorecover_settings_panel" | "bookmark_panel" | "caption_panel" | "cell_link_panel" | "cell_size_panel" | "cells_dialog_panel" | "chart_panel" | "chart_template_panel" | "color_panel" | "compare_panel" | "compress_panel" | "consolidate_panel" | "control_panel" | "control_properties_panel" | "crop_panel" | "cross_reference_panel" | "custom_views_panel" | "data_table_panel" | "dedupe_panel" | "doc_autocorrect_panel" | "doc_compatibility_panel" | "doc_inspect_panel" | "doc_page_setup_panel" | "doc_print_panel" | "doc_properties_panel" | "doc_repair_panel" | "doc_source_manager_panel" | "doc_template_panel" | "docx_compress_panel" | "docx_math_panel" | "docx_theme_panel" | "error_options_panel" | "external_links_panel" | "field_panel" | "font_panel" | "forecast_panel" | "frame_panel" | "glow_panel" | "goal_seek_panel" | "goto_special_panel" | "header_footer_panel" | "import_text_panel" | "kinsoku_panel" | "layout_name_panel" | "link_panel" | "list_style_panel" | "move_chart_panel" | "new_cell_style_panel" | "new_style_panel" | "new_table_style_panel" | "notes_marks_panel" | "notes_panel" | "page_setup_panel" | "paragraph_panel" | "paste_special_panel" | "phonetic_panel" | "photo_album_panel" | "pivot_calc_field_panel" | "pivot_calc_item_panel" | "pivot_group_panel" | "pivot_options_panel" | "pivot_panel" | "pptx_autocorrect_panel" | "pptx_compatibility_panel" | "pptx_gradient_panel" | "pptx_math_panel" | "pptx_paragraph_panel" | "pptx_section_panel" | "pptx_trust_center_panel" | "presentation_inspect_panel" | "presentation_properties_panel" | "print_panel" | "protect_panel" | "protect_workbook_panel" | "queries_panel" | "recommended_charts_panel" | "recover_unsaved_panel" | "replace_fonts_panel" | "row_col_size_panel" | "ruby_panel" | "scenario_manager_panel" | "series_panel" | "shadow_panel" | "sheet_doc_properties_panel" | "sheet_spelling_panel" | "sheet_word_count_panel" | "show_panel" | "slicer_insert_panel" | "slicer_settings_panel" | "slide_link_panel" | "slide_size_panel" | "solver_panel" | "sparkline_panel" | "style_panel" | "subtotal_panel" | "symbol_panel" | "table_formula_panel" | "table_size_panel" | "text_columns_panel" | "text_insets_panel" | "theme_panel" | "timeline_insert_panel" | "timing_panel" | "unhide_window_panel" | "update_toc_panel" | "user_info_panel" | "validation_panel" | "watermark_panel" | "xlsx_backup_save_settings_panel" | "xlsx_compatibility_panel" | "xlsx_repair_panel" | "xlsx_theme_panel" | "xml_map_properties_panel" => PanelMode::Modal,
        "evaluate_formula_panel" | "function_panel" | "go_to_panel" | "watch_window_panel" => PanelMode::Modeless,
        _ => PanelMode::Sidebar,
    };
    (key, mode)
}

/// Valid recovery files in an isolated fixture directory; never changes the user's recovery folder.
fn seed_recovery_gallery(e:&mut crate::shell::editor::SlideEditor) {
    let root=crate::shell::scenario::repo_root();
    let dir=root.join("target/panel-gallery-recovery-fixture");std::fs::create_dir_all(&dir).unwrap();
    for name in ["Quarterly-report-autorecovered.docx","Meeting-notes-autorecovered.docx"]{
        std::fs::copy(root.join("test/editing/docx-report.docx"),dir.join(name)).unwrap();
    }
    if let Some(p)=e.recover_unsaved.as_mut(){p.entries=crate::shell::autorecover::scan_recovered_files(&dir);}
}

fn seed_autocorrect_entry(e:&mut crate::shell::editor::SlideEditor,w:&mut gpui_kit::Window,cx:&mut gpui_kit::Context<crate::shell::editor::SlideEditor>) {
    let p=match e.format.kind(){ibis_pptx::Kind::Document=>e.doc_state_mut().unwrap().autocorrect.as_mut().unwrap(),ibis_pptx::Kind::Workbook=>e.wb_mut().panels.autocorrect.as_mut().unwrap(),_=>e.pptx_mut().unwrap().autocorrect.as_mut().unwrap()};
    p.entry_open=true;p.editing=Some(0);let (a,b)=p.working.table[0].clone();p.replace.update(cx,|i,cx|i.set_value(a,w,cx));p.with.update(cx,|i,cx|i.set_value(b,w,cx));
}

fn seed_watch_gallery(e:&mut crate::shell::editor::SlideEditor){
 let slide=e.current_slide;let view=ibis_doc_model::BookView::new(e.presentation());let name=ibis_calc::eval::Book::sheet_name(&view,view.index_of_slide(slide).unwrap_or(0)).to_string();
 e.wb_mut().panels.watches=(0..3).map(|i|crate::xlsx::formula_audit::WatchEntry{id:i+1,slide_id:slide,sheet_name:name.clone(),col:i as u32,row:1,cell_ref:ibis_doc_model::cell_name(i as u32,1),name:None,value_text:format!("{}",(i+1)*100),formula_text:None}).collect();
}

fn seed_pptx_gallery_shapes(e:&mut crate::shell::editor::SlideEditor,cx:&mut gpui_kit::Context<crate::shell::editor::SlideEditor>)->Vec<ibis_doc_model::ShapeId>{
 use ibis_doc_model::*;
 while e.slide().shapes.len()<3{
  let id=e.document.ids().new_shape_id();let mut a=AutoShape::new(Geometry::Rect,Fill::Solid(Color::opaque(Rgb(0x4472c4))),None);
  a.text=Some(TextBody::plain("Supplement",TextStyle::default()));e.insert_shape(Shape::new(id,EmuRect::new(900000,3500000,3000000,900000),ShapeKind::Auto(a)),cx);
 }
 let ids=e.slide().shapes.iter().take(3).map(|s|s.id).collect::<Vec<_>>();let slide=e.current_slide;
 e.run(ids.iter().zip(["Title","Overview","Supplement"]).map(|(&shape,name)|Command::SetShapeName{slide,shape,name:name.into()}).collect(),cx);
 e.set_selection(vec![ids[0]]);ids
}
