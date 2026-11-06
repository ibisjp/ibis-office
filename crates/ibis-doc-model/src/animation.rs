// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A slide's animations (アニメーション, `p:timing`), as the slide show
//! plays them: the effects of its main sequence, in order. They are read
//! from the file and played; the file's timing is saved as it was.

use crate::ShapeId;

/// What an effect does to its shape (`presetClass`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationClass {
    /// 開始: the shape is hidden until the effect brings it in.
    Entrance,
    /// 強調: the shape stays.
    Emphasis,
    /// 終了: the shape is hidden after the effect.
    Exit,
    /// アニメーションの軌跡: the shape moves along a path.
    Path,
    /// メディア (`presetClass="mediacall"`): the effect does not move its shape, it plays, pauses or stops the video or
    /// audio the shape holds ([`AnimationLook::Media`]).
    Media,
}

/// What a メディア effect does to its clip (the `p:cmd` of the effect).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    /// 再生 (`playFrom(..)`, preset 1).
    Play,
    /// 一時停止 (`togglePause`, preset 2): pauses a playing clip and resumes a paused one.
    Pause,
    /// 停止 (`stop`, preset 3).
    Stop,
}

/// When an effect starts (`nodeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationStart {
    /// クリック時.
    OnClick,
    /// 直前の動作と同時.
    WithPrevious,
    /// 直前の動作の後.
    AfterPrevious,
}

/// A side of the slide an effect comes in from or goes out to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationSide {
    Top,
    Right,
    Bottom,
    Left,
}

/// How an effect looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationLook {
    /// アピール: at once.
    Appear,
    /// フェード, and the effects drawn as a fade until they are drawn
    /// themselves (ワイプ, ズーム…).
    Fade,
    /// スライドイン: from (to, for an exit) a side of the slide.
    Fly(AnimationSide),
    /// スピン: turned by this many 1/60000 degrees.
    Spin(i32),
    /// 拡大／収縮: to these sizes, 1/1000 % (100 000 is the shape's size).
    Scale(i32, i32),
    /// A path ending this far from the shape, in 1/100 000 of the slide's
    /// width and height.
    Motion(i32, i32),
    /// ズーム: scales from/to center with fade.
    Zoom,
    /// フロート: smooth rising or sinking slide with fade.
    Float(AnimationSide),
    /// ワイプ: wipe from a side of the slide.
    Wipe(AnimationSide),
    /// インクの再生 (Replay): animates an ink shape.
    Replay,
    /// Any other 開始・終了 effect (PowerPoint's presets by `presetID`): played as the effect's own behaviours
    /// ([`Animation::behaviors`]) say.
    Preset,
    /// 再生・一時停止・停止 of a video or an audio ([`AnimationClass::Media`]).
    Media(MediaCommand),
    /// Anything else: shown as the class says, without movement.
    Other,
}

/// What a text effect goes by when it takes the text a word or a letter at a time (`p:iterate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextUnit {
    /// 単語 (`type="wd"`).
    Word,
    /// 文字 (`type="lt"`).
    Letter,
}

/// How long after one word or letter the next begins: a share of the effect's time (`tmPct`, in 1/1000 %) or a time (`tmAbs`, ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitGap {
    Percent(u32),
    Millis(u32),
}

/// How many times an animation effect repeats (繰返し, `repeatCount`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationRepeat {
    /// 1 time (no repetition, 属性省略).
    #[default]
    None,
    /// Specific count of times (e.g. 2, 3, 4, 5, 10).
    Count(u32),
    /// Until next click (`"indefinite"`).
    UntilNextClick,
    /// Until end of slide (`"indefinite"`).
    UntilEndOfSlide,
}

/// Behavior of the shape after animation finishes (効果終了時の動作, `fill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnimationAfterEffect {
    /// 消去しない (Don't dim / hold final state, `fill="hold"`).
    #[default]
    Hold,
    /// 再生後に非表示 (Hide after animation, `fill="remove"`).
    Hide,
    /// 次のクリック時に非表示 (Hide on next click).
    HideOnNextClick,
    /// 淡色表示 (Dim with color).
    Dim,
}

/// What triggers an animation effect (アニメーションのトリガー).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AnimationTrigger {
    /// Normal slide sequence (通常のスライドクリック・自動進行で再生).
    #[default]
    None,
    /// オブジェクトのクリック時: plays when this shape is clicked in the slide show.
    OnShapeClick(ShapeId),
    /// メディアブックマーク時: plays when this media shape's bookmark is reached.
    OnMediaBookmark {
        shape: ShapeId,
        bookmark: String,
    },
}

impl AnimationTrigger {
    /// Whether this is a normal untriggered animation.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The target shape associated with this trigger, if any.
    pub fn trigger_shape(&self) -> Option<ShapeId> {
        match self {
            Self::OnShapeClick(s) => Some(*s),
            Self::OnMediaBookmark { shape, .. } => Some(*shape),
            Self::None => None,
        }
    }
}

/// One effect of a slide's animations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    /// The shape it moves.
    pub shape: ShapeId,
    pub class: AnimationClass,
    pub start: AnimationStart,
    pub look: AnimationLook,
    /// When it starts after its start (`delay`), and how long it takes,
    /// milliseconds.
    pub delay: u32,
    pub duration: u32,
    /// PowerPoint's preset (`presetID`, `presetSubtype`), as read.
    pub preset: (u32, u32),
    /// How many times the animation repeats (`repeatCount`).
    pub repeat_count: AnimationRepeat,
    /// Whether the animation auto-reverses on completion (`autoRev="1"`).
    pub auto_reverse: bool,
    /// Whether the animation rewinds to starting state when done playing (`rewind="1"`).
    pub rewind: bool,
    /// Acceleration rate (滑らかに開始, `accel`), in 1/1000 % (0 to 100 000).
    pub accel: u32,
    /// Deceleration rate (滑らかに終了, `decel`), in 1/1000 % (0 to 100 000).
    pub decel: u32,
    /// Behavior after animation finishes (`fill`).
    pub after_effect: AnimationAfterEffect,
    /// Trigger that starts this animation (特定の図形クリックまたはメディアブックマーク).
    pub trigger: AnimationTrigger,
    /// The paragraphs of the shape's text it moves (`p:txEl/p:pRg`: first
    /// and last, from 0); `None` moves the whole shape.
    pub paragraph: Option<(u32, u32)>,
    /// The characters of the shape's text it moves (`p:txEl/p:charRg`: first and last, counted over the whole text, a paragraph
    /// break as one; by word or by letter an effect for each); `None` when it moves the whole shape or paragraphs.
    pub chars: Option<(u32, u32)>,
    /// The text taken a word or a letter at a time (`p:iterate`), and the gap between them.
    pub text_unit: Option<(TextUnit, UnitGap)>,
    /// カスタム軌跡: the points a motion path goes through after its start
    /// (0, 0), as offsets from the shape in 1/100 000 of the slide's width
    /// and height, joined by straight lines. Empty for a path that is the
    /// one line to [`AnimationLook::Motion`]'s end; otherwise the last
    /// point is that end.
    pub path: Vec<(i32, i32)>,
    /// A chart element this effect targets (`p:cEl/p:cChartEl`: series and category indices);
    /// `None` when it moves the whole shape or text.
    pub chart_element: Option<ChartAnimationElement>,
    /// The chart build step and anim_bg setting (`p:bldChart`).
    pub chart_build: Option<(ChartBuildStep, bool)>,
    /// A diagram element this effect targets (`p:graphicEl/a:dgm` or `p:subSp`);
    /// `None` when it moves the whole shape, text, or chart.
    pub dgm_element: Option<DiagramAnimationElement>,
    /// The diagram build step (`p:bldDgm`).
    pub dgm_build: Option<DiagramBuildStep>,
    /// The effect's behaviours (`p:set`, `p:anim`, `p:animEffect`…) for [`AnimationLook::Preset`]; empty for the others.
    pub behaviors: Vec<crate::animation_preset::AnimBehavior>,
}

/// How a chart's elements are built in animation (`ST_ChartBuildStep` in ECMA-376 Part 1 19.5.15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartBuildStep {
    #[default]
    AllAtOnce,
    Series,
    Category,
    SeriesEl,
    CategoryEl,
}

impl ChartBuildStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AllAtOnce => "allAtOnce",
            Self::Series => "series",
            Self::Category => "category",
            Self::SeriesEl => "seriesEl",
            Self::CategoryEl => "categoryEl",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allAtOnce" => Some(Self::AllAtOnce),
            "series" => Some(Self::Series),
            "category" => Some(Self::Category),
            "seriesEl" => Some(Self::SeriesEl),
            "categoryEl" => Some(Self::CategoryEl),
            _ => None,
        }
    }
}

/// A chart animation target element (`p:cChartEl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ChartAnimationElement {
    /// Target series index, or -1 for background / all series.
    pub series_idx: i32,
    /// Target category index, or -1 for background / all categories.
    pub category_idx: i32,
}

/// How a diagram's (SmartArt's) elements are built in animation (`ST_DiagramBuildStep` in ECMA-376 Part 1 19.5.17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiagramBuildStep {
    #[default]
    Whole,
    DepthByNode,
    DepthByBranch,
    BreadthByNode,
    BreadthByLvl,
    AllAtOnce,
    Cust,
}

impl DiagramBuildStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::DepthByNode => "depthByNode",
            Self::DepthByBranch => "depthByBranch",
            Self::BreadthByNode => "breadthByNode",
            Self::BreadthByLvl => "breadthByLvl",
            Self::AllAtOnce => "allAtOnce",
            Self::Cust => "cust",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "whole" => Some(Self::Whole),
            "depthByNode" => Some(Self::DepthByNode),
            "depthByBranch" => Some(Self::DepthByBranch),
            "breadthByNode" => Some(Self::BreadthByNode),
            "breadthByLvl" => Some(Self::BreadthByLvl),
            "allAtOnce" => Some(Self::AllAtOnce),
            "cust" => Some(Self::Cust),
            _ => None,
        }
    }
}

/// A diagram animation target element (`p:graphicEl/a:dgm` or `p:subSp`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DiagramAnimationElement {
    /// Target diagram element id (modelId or sub-shape id).
    pub id: String,
    /// Build step type (e.g. "sp" for shape, "bg" for background).
    pub bld_step: String,
}

impl Animation {
    /// Whether it plays, pauses or stops a clip (class メディア) instead of moving its shape.
    pub fn is_media(&self) -> bool {
        self.class == AnimationClass::Media
    }

    /// The clip command of a メディア effect.
    pub fn media_command(&self) -> Option<MediaCommand> {
        match self.look {
            AnimationLook::Media(c) => Some(c),
            _ => None,
        }
    }

    /// Whether the animation has smooth start acceleration (`accel > 0`).
    pub fn smooth_start(&self) -> bool {
        self.accel > 0
    }

    /// Whether the animation has smooth end deceleration (`decel > 0`).
    pub fn smooth_end(&self) -> bool {
        self.decel > 0
    }

    /// Whether the animation is triggered by a shape click or media bookmark.
    pub fn is_triggered(&self) -> bool {
        !self.trigger.is_none()
    }
}


