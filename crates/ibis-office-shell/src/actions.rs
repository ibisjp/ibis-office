// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The app's commands (GPUI actions, in the namespace `ibis_office`): the menus, the keys and the formats name them.

use gpui_kit::*;

actions!(
    ibis_office,
    [
        AddPresentationSlide,
        DuplicatePresentationSelection,
        /// Undo the last edit.
        Undo,
        /// Redo the last undone edit.
        Redo,
        /// Delete the selected shape.
        DeleteSelection,
        /// Leave text editing, the insert tool, or the selection, in that order.
        CancelTool,
        /// Open a PowerPoint, Excel or Word file.
        OpenFile,
        /// Make a new, empty Word document (ファイル > 新規作成 > ドキュメント).
        NewDocument,
        /// Open the start window (ファイル > スタートウィンドウ).
        OpenStartWindow,
        /// Make a new presentation from a template (ファイル > 新規 > テンプレートから新規作成…).
        NewFromTemplate,
        /// Close the open file and show the start screen (スタート画面).
        CloseFile,
        /// Save the open file (to the file it was opened from).
        Save,
        /// Save the open file under another name.
        SaveAs,
        /// Print the open file (ファイル > 印刷…, ⌘P).
        PrintFile,
        /// Hide or show the title row and the menu bar (the toolbar's ^).
        ToggleMenus,
        /// Show the next slide.
        NextSlide,
        /// Show the previous slide.
        PreviousSlide,
        /// Show the first slide.
        FirstSlide,
        /// Show the last slide.
        LastSlide,
        /// Start the slide show from the slide shown.
        StartSlideshow,
        /// Start the slide show from the first slide.
        StartSlideshowFromBeginning,
        /// Start the slide show from the slide shown with the presenter's
        /// view (発表者ビュー).
        StartPresenterView,
        InsertComment,
        /// 挿入 > リンク (a presentation's hyperlink).
        InsertHyperlink,
        /// Black out the slide show, or show it again.
        BlackScreen,
        /// White out the slide show, or show it again.
        WhiteScreen,
        /// Quit the application.
        Quit,
        /// Open the settings window (ibisOffice > 設定…, ⌘,).
        OpenSettings,
        /// Open the About window (ibisOffice > ibisOffice について).
        OpenAbout,
        /// Select the cell above the selected one on a worksheet.
        SelectCellUp,
        /// Select the cell below the selected one on a worksheet.
        SelectCellDown,
        /// Select the cell to the left of the selected one on a worksheet.
        SelectCellLeft,
        /// Select the cell to the right of the selected one on a worksheet.
        SelectCellRight,
        /// Show the next worksheet of a workbook.
        NextSheet,
        /// Show the previous worksheet of a workbook.
        PreviousSheet,
        /// Stretch the selected range of a worksheet a cell up.
        ExtendCellsUp,
        /// Stretch the selected range of a worksheet a cell down.
        ExtendCellsDown,
        /// Stretch the selected range of a worksheet a cell to the left.
        ExtendCellsLeft,
        /// Stretch the selected range of a worksheet a cell to the right.
        ExtendCellsRight,
        /// Scroll a document up by a few lines.
        ScrollLineUp,
        /// Scroll a document down by a few lines.
        ScrollLineDown,
        /// Scroll to the start of a document.
        ScrollToStart,
        /// Scroll to the end of a document.
        ScrollToEnd,
        /// Switch to laser pointer mode in the slide show.
        LaserPointer,
        /// Switch to pen drawing mode in the slide show.
        PenPointer,
        /// Switch to highlighter drawing mode in the slide show.
        HighlighterPointer,
        /// Switch back to arrow pointer mode in the slide show.
        ArrowPointer,
        /// Switch to eraser mode in the slide show.
        EraserPointer,
        /// Clear all ink annotations on the current slide.
        ClearSlideInks,
        /// Toggle the all slides grid overview in the slide show.
        ToggleSlideshowGrid,
        /// Zoom in the slide show view.
        ZoomInSlideshow,
        /// Zoom out the slide show view.
        ZoomOutSlideshow,
        /// Reset zoom in the slide show view to normal 1.0x.
        ResetSlideshowZoom,
        /// Start rehearsing slide show timings.
        RehearseTimings,
        /// 表示 > 拡大・縮小・100%・ウィンドウに合わせる（⌘+・⌘-・⌘0・⌥⌘0）.
        ZoomInView,
        ZoomOutView,
        ZoomActualView,
        ZoomFitView,
        /// Toggle the slide sorter grid view in the presentation editor.
        ToggleSlideGridView,
        /// Start or exit reading view in the presentation editor.
        ToggleReadingView,
        /// Switch to normal view in the presentation editor.
        SwitchToNormalView,
        /// Toggle the outline view in the presentation editor.
        ToggleOutlineView,
        /// Switch to outline view in the presentation editor.
        SwitchToOutlineView,
        /// Toggle the notes page view in the presentation editor.
        ToggleNotesPageView,
        /// Switch to notes page view in the presentation editor.
        SwitchToNotesPageView,
        /// Open the zoom dialog.
        OpenZoomDialog,
        /// Open a new window duplicating current presentation or empty.
        NewEditorWindow,
        /// The list of open windows (Ctrl+Tab).
        WindowList,
        /// The list of open windows, current on the last row (Ctrl+Shift+Tab).
        WindowListBack,
        /// The next window in the order of opening.
        NextWindow,
        /// The previous window in the order of opening.
        PreviousWindow,
        /// Arrange all editor windows evenly.
        ArrangeAllWindows,
        /// Cascade all editor windows.
        CascadeWindows,
        /// Copy animations of selected shape (Animation Painter).
        CopyAnimation,
        /// Paste animations onto selected shapes (Animation Painter).
        PasteAnimation,
    ]
);
