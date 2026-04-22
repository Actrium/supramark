//! Mermaid theme system — port of upstream `themes/*.js`.
//!
//! Each upstream theme is flattened into a single [`ThemeVariables`]
//! struct whose field names mirror the JS variable names (converted to
//! `snake_case`). Every field is `Option` so that a partially-populated
//! user override can be merged on top of a built-in variant without
//! losing the unset defaults.
//!
//! ## Upstream mapping
//!
//! Upstream (`packages/mermaid/src/themes/theme-*.js`) builds each
//! theme by:
//!
//! 1. Assigning literal seed colors in a `Theme` class constructor.
//! 2. Running `updateColors()`, which derives the rest via
//!    `khroma.adjust / lighten / darken / invert`.
//!
//! We resolve both steps ahead of time by importing the upstream JS,
//! calling `getThemeVariables()`, and capturing the resulting flat
//! object as literal string / numeric constants. The Rust setter
//! functions in `theme::{default, base, dark, forest, neutral}` simply
//! write those constants into a `ThemeVariables`.
//!
//! This keeps Wave 0 free of any runtime dependency on the color math
//! in `theme::color` (owned by Agent A), but when that module lands we
//! can optionally switch the larger palettes (e.g. `cScale*`) to
//! runtime computation for byte-exact parity with an override that
//! changes `primaryColor`.

pub mod base;
pub mod color;
pub mod css;
pub mod dark;
pub mod default;
pub mod forest;
pub mod neutral;

/// Radar chart theme subgroup (upstream: `theme.radar = { ... }`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RadarVars {
    pub axis_color: Option<String>,          // radar.axisColor
    pub axis_label_font_size: Option<i64>,   // radar.axisLabelFontSize
    pub axis_stroke_width: Option<i64>,      // radar.axisStrokeWidth
    pub curve_opacity: Option<f64>,          // radar.curveOpacity
    pub curve_stroke_width: Option<i64>,     // radar.curveStrokeWidth
    pub graticule_color: Option<String>,     // radar.graticuleColor
    pub graticule_opacity: Option<f64>,      // radar.graticuleOpacity
    pub graticule_stroke_width: Option<i64>, // radar.graticuleStrokeWidth
    pub legend_box_size: Option<i64>,        // radar.legendBoxSize
    pub legend_font_size: Option<i64>,       // radar.legendFontSize
}

/// XY chart theme subgroup (upstream: `theme.xyChart = { ... }`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XyChartVars {
    pub background_color: Option<String>, // xyChart.backgroundColor
    pub data_label_color: Option<String>, // xyChart.dataLabelColor
    pub plot_color_palette: Option<String>, // xyChart.plotColorPalette
    pub title_color: Option<String>,      // xyChart.titleColor
    pub x_axis_label_color: Option<String>, // xyChart.xAxisLabelColor
    pub x_axis_line_color: Option<String>, // xyChart.xAxisLineColor
    pub x_axis_tick_color: Option<String>, // xyChart.xAxisTickColor
    pub x_axis_title_color: Option<String>, // xyChart.xAxisTitleColor
    pub y_axis_label_color: Option<String>, // xyChart.yAxisLabelColor
    pub y_axis_line_color: Option<String>, // xyChart.yAxisLineColor
    pub y_axis_tick_color: Option<String>, // xyChart.yAxisTickColor
    pub y_axis_title_color: Option<String>, // xyChart.yAxisTitleColor
}

/// Packet diagram theme subgroup (upstream: `theme.packet = { ... }`).
/// Only the `dark` and `forest` themes set this; other themes leave it `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PacketVars {
    pub block_fill_color: Option<String>,   // packet.blockFillColor
    pub block_stroke_color: Option<String>, // packet.blockStrokeColor
    pub end_byte_color: Option<String>,     // packet.endByteColor
    pub label_color: Option<String>,        // packet.labelColor
    pub start_byte_color: Option<String>,   // packet.startByteColor
    pub title_color: Option<String>,        // packet.titleColor
}

/// Flat collection of every upstream theme variable.
///
/// All fields are `Option` so an instance can represent either a
/// complete built-in theme or a partial user override. To layer two
/// instances, use [`ThemeVariables::merge`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeVariables {
    pub theme_color_limit: Option<i64>,           // THEME_COLOR_LIMIT
    pub activation_bkg_color: Option<String>,     // activationBkgColor
    pub activation_border_color: Option<String>,  // activationBorderColor
    pub active_task_bkg_color: Option<String>,    // activeTaskBkgColor
    pub active_task_border_color: Option<String>, // activeTaskBorderColor
    pub actor_bkg: Option<String>,                // actorBkg
    pub actor_border: Option<String>,             // actorBorder
    pub actor_line_color: Option<String>,         // actorLineColor
    pub actor_text_color: Option<String>,         // actorTextColor
    pub alt_background: Option<String>,           // altBackground
    pub alt_section_bkg_color: Option<String>,    // altSectionBkgColor
    pub arch_edge_arrow_color: Option<String>,    // archEdgeArrowColor
    pub arch_edge_color: Option<String>,          // archEdgeColor
    pub arch_edge_width: Option<String>,          // archEdgeWidth
    pub arch_group_border_color: Option<String>,  // archGroupBorderColor
    pub arch_group_border_width: Option<String>,  // archGroupBorderWidth
    pub arrowhead_color: Option<String>,          // arrowheadColor
    pub attribute_background_color_even: Option<String>, // attributeBackgroundColorEven
    pub attribute_background_color_odd: Option<String>, // attributeBackgroundColorOdd
    pub background: Option<String>,               // background
    pub border1: Option<String>,                  // border1
    pub border2: Option<String>,                  // border2
    pub branch_label_color: Option<String>,       // branchLabelColor

    pub c_scale0: Option<String>,  // cScale0
    pub c_scale1: Option<String>,  // cScale1
    pub c_scale2: Option<String>,  // cScale2
    pub c_scale3: Option<String>,  // cScale3
    pub c_scale4: Option<String>,  // cScale4
    pub c_scale5: Option<String>,  // cScale5
    pub c_scale6: Option<String>,  // cScale6
    pub c_scale7: Option<String>,  // cScale7
    pub c_scale8: Option<String>,  // cScale8
    pub c_scale9: Option<String>,  // cScale9
    pub c_scale10: Option<String>, // cScale10
    pub c_scale11: Option<String>, // cScale11
    pub c_scale12: Option<String>, // cScale12

    pub c_scale_inv0: Option<String>,  // cScaleInv0
    pub c_scale_inv1: Option<String>,  // cScaleInv1
    pub c_scale_inv2: Option<String>,  // cScaleInv2
    pub c_scale_inv3: Option<String>,  // cScaleInv3
    pub c_scale_inv4: Option<String>,  // cScaleInv4
    pub c_scale_inv5: Option<String>,  // cScaleInv5
    pub c_scale_inv6: Option<String>,  // cScaleInv6
    pub c_scale_inv7: Option<String>,  // cScaleInv7
    pub c_scale_inv8: Option<String>,  // cScaleInv8
    pub c_scale_inv9: Option<String>,  // cScaleInv9
    pub c_scale_inv10: Option<String>, // cScaleInv10
    pub c_scale_inv11: Option<String>, // cScaleInv11

    pub c_scale_label0: Option<String>,  // cScaleLabel0
    pub c_scale_label1: Option<String>,  // cScaleLabel1
    pub c_scale_label2: Option<String>,  // cScaleLabel2
    pub c_scale_label3: Option<String>,  // cScaleLabel3
    pub c_scale_label4: Option<String>,  // cScaleLabel4
    pub c_scale_label5: Option<String>,  // cScaleLabel5
    pub c_scale_label6: Option<String>,  // cScaleLabel6
    pub c_scale_label7: Option<String>,  // cScaleLabel7
    pub c_scale_label8: Option<String>,  // cScaleLabel8
    pub c_scale_label9: Option<String>,  // cScaleLabel9
    pub c_scale_label10: Option<String>, // cScaleLabel10
    pub c_scale_label11: Option<String>, // cScaleLabel11

    pub c_scale_peer0: Option<String>,  // cScalePeer0
    pub c_scale_peer1: Option<String>,  // cScalePeer1
    pub c_scale_peer2: Option<String>,  // cScalePeer2
    pub c_scale_peer3: Option<String>,  // cScalePeer3
    pub c_scale_peer4: Option<String>,  // cScalePeer4
    pub c_scale_peer5: Option<String>,  // cScalePeer5
    pub c_scale_peer6: Option<String>,  // cScalePeer6
    pub c_scale_peer7: Option<String>,  // cScalePeer7
    pub c_scale_peer8: Option<String>,  // cScalePeer8
    pub c_scale_peer9: Option<String>,  // cScalePeer9
    pub c_scale_peer10: Option<String>, // cScalePeer10
    pub c_scale_peer11: Option<String>, // cScalePeer11

    pub class_text: Option<String>,                 // classText
    pub cluster_bkg: Option<String>,                // clusterBkg
    pub cluster_border: Option<String>,             // clusterBorder
    pub commit_label_background: Option<String>,    // commitLabelBackground
    pub commit_label_color: Option<String>,         // commitLabelColor
    pub commit_label_font_size: Option<String>,     // commitLabelFontSize
    pub composite_background: Option<String>,       // compositeBackground
    pub composite_border: Option<String>,           // compositeBorder
    pub composite_title_background: Option<String>, // compositeTitleBackground
    pub contrast: Option<String>,                   // contrast
    pub crit_bkg_color: Option<String>,             // critBkgColor
    pub crit_border_color: Option<String>,          // critBorderColor
    pub critical: Option<String>,                   // critical
    pub dark_text_color: Option<String>,            // darkTextColor
    pub default_link_color: Option<String>,         // defaultLinkColor
    pub done: Option<String>,                       // done
    pub done_task_bkg_color: Option<String>,        // doneTaskBkgColor
    pub done_task_border_color: Option<String>,     // doneTaskBorderColor
    pub drop_shadow: Option<String>,                // dropShadow
    pub edge_label_background: Option<String>,      // edgeLabelBackground
    pub error_bkg_color: Option<String>,            // errorBkgColor
    pub error_text_color: Option<String>,           // errorTextColor
    pub exclude_bkg_color: Option<String>,          // excludeBkgColor

    pub fill_type0: Option<String>, // fillType0
    pub fill_type1: Option<String>, // fillType1
    pub fill_type2: Option<String>, // fillType2
    pub fill_type3: Option<String>, // fillType3
    pub fill_type4: Option<String>, // fillType4
    pub fill_type5: Option<String>, // fillType5
    pub fill_type6: Option<String>, // fillType6
    pub fill_type7: Option<String>, // fillType7

    pub font_family: Option<String>, // fontFamily
    pub font_size: Option<String>,   // fontSize
    pub font_weight: Option<String>, // fontWeight

    pub git0: Option<String>,              // git0
    pub git1: Option<String>,              // git1
    pub git2: Option<String>,              // git2
    pub git3: Option<String>,              // git3
    pub git4: Option<String>,              // git4
    pub git5: Option<String>,              // git5
    pub git6: Option<String>,              // git6
    pub git7: Option<String>,              // git7
    pub git_branch_label0: Option<String>, // gitBranchLabel0
    pub git_branch_label1: Option<String>, // gitBranchLabel1
    pub git_branch_label2: Option<String>, // gitBranchLabel2
    pub git_branch_label3: Option<String>, // gitBranchLabel3
    pub git_branch_label4: Option<String>, // gitBranchLabel4
    pub git_branch_label5: Option<String>, // gitBranchLabel5
    pub git_branch_label6: Option<String>, // gitBranchLabel6
    pub git_branch_label7: Option<String>, // gitBranchLabel7
    pub git_inv0: Option<String>,          // gitInv0
    pub git_inv1: Option<String>,          // gitInv1
    pub git_inv2: Option<String>,          // gitInv2
    pub git_inv3: Option<String>,          // gitInv3
    pub git_inv4: Option<String>,          // gitInv4
    pub git_inv5: Option<String>,          // gitInv5
    pub git_inv6: Option<String>,          // gitInv6
    pub git_inv7: Option<String>,          // gitInv7

    pub gradient_start: Option<String>,         // gradientStart
    pub gradient_stop: Option<String>,          // gradientStop
    pub grid_color: Option<String>,             // gridColor
    pub inner_end_background: Option<String>,   // innerEndBackground
    pub label_background: Option<String>,       // labelBackground
    pub label_background_color: Option<String>, // labelBackgroundColor
    pub label_box_bkg_color: Option<String>,    // labelBoxBkgColor
    pub label_box_border_color: Option<String>, // labelBoxBorderColor
    pub label_color: Option<String>,            // labelColor
    pub label_text_color: Option<String>,       // labelTextColor
    pub line_color: Option<String>,             // lineColor
    pub loop_text_color: Option<String>,        // loopTextColor
    pub main_bkg: Option<String>,               // mainBkg
    pub main_contrast_color: Option<String>,    // mainContrastColor
    pub node_bkg: Option<String>,               // nodeBkg
    pub node_border: Option<String>,            // nodeBorder
    pub node_text_color: Option<String>,        // nodeTextColor
    pub note: Option<String>,                   // note
    pub note_bkg_color: Option<String>,         // noteBkgColor
    pub note_border_color: Option<String>,      // noteBorderColor
    pub note_font_weight: Option<String>,       // noteFontWeight
    pub note_text_color: Option<String>,        // noteTextColor
    pub person_bkg: Option<String>,             // personBkg
    pub person_border: Option<String>,          // personBorder

    pub pie0: Option<String>,                   // pie0
    pub pie1: Option<String>,                   // pie1
    pub pie2: Option<String>,                   // pie2
    pub pie3: Option<String>,                   // pie3
    pub pie4: Option<String>,                   // pie4
    pub pie5: Option<String>,                   // pie5
    pub pie6: Option<String>,                   // pie6
    pub pie7: Option<String>,                   // pie7
    pub pie8: Option<String>,                   // pie8
    pub pie9: Option<String>,                   // pie9
    pub pie10: Option<String>,                  // pie10
    pub pie11: Option<String>,                  // pie11
    pub pie12: Option<String>,                  // pie12
    pub pie_legend_text_color: Option<String>,  // pieLegendTextColor
    pub pie_legend_text_size: Option<String>,   // pieLegendTextSize
    pub pie_opacity: Option<String>,            // pieOpacity
    pub pie_outer_stroke_color: Option<String>, // pieOuterStrokeColor
    pub pie_outer_stroke_width: Option<String>, // pieOuterStrokeWidth
    pub pie_section_text_color: Option<String>, // pieSectionTextColor
    pub pie_section_text_size: Option<String>,  // pieSectionTextSize
    pub pie_stroke_color: Option<String>,       // pieStrokeColor
    pub pie_stroke_width: Option<String>,       // pieStrokeWidth
    pub pie_title_text_color: Option<String>,   // pieTitleTextColor
    pub pie_title_text_size: Option<String>,    // pieTitleTextSize

    pub primary_border_color: Option<String>, // primaryBorderColor
    pub primary_color: Option<String>,        // primaryColor
    pub primary_text_color: Option<String>,   // primaryTextColor

    pub quadrant1_fill: Option<String>,      // quadrant1Fill
    pub quadrant1_text_fill: Option<String>, // quadrant1TextFill
    pub quadrant2_fill: Option<String>,      // quadrant2Fill
    pub quadrant2_text_fill: Option<String>, // quadrant2TextFill
    pub quadrant3_fill: Option<String>,      // quadrant3Fill
    pub quadrant3_text_fill: Option<String>, // quadrant3TextFill
    pub quadrant4_fill: Option<String>,      // quadrant4Fill
    pub quadrant4_text_fill: Option<String>, // quadrant4TextFill
    pub quadrant_external_border_stroke_fill: Option<String>, // quadrantExternalBorderStrokeFill
    pub quadrant_internal_border_stroke_fill: Option<String>, // quadrantInternalBorderStrokeFill
    pub quadrant_point_fill: Option<String>, // quadrantPointFill
    pub quadrant_point_text_fill: Option<String>, // quadrantPointTextFill
    pub quadrant_title_fill: Option<String>, // quadrantTitleFill
    pub quadrant_x_axis_text_fill: Option<String>, // quadrantXAxisTextFill
    pub quadrant_y_axis_text_fill: Option<String>, // quadrantYAxisTextFill

    pub radius: Option<i64>,                       // radius
    pub relation_color: Option<String>,            // relationColor
    pub relation_label_background: Option<String>, // relationLabelBackground
    pub relation_label_color: Option<String>,      // relationLabelColor
    pub requirement_background: Option<String>,    // requirementBackground
    pub requirement_border_color: Option<String>,  // requirementBorderColor
    pub requirement_border_size: Option<String>,   // requirementBorderSize
    pub requirement_text_color: Option<String>,    // requirementTextColor
    pub row_even: Option<String>,                  // rowEven
    pub row_odd: Option<String>,                   // rowOdd
    pub scale_label_color: Option<String>,         // scaleLabelColor
    pub second_bkg: Option<String>,                // secondBkg
    pub secondary_border_color: Option<String>,    // secondaryBorderColor
    pub secondary_color: Option<String>,           // secondaryColor
    pub secondary_text_color: Option<String>,      // secondaryTextColor
    pub section_bkg_color: Option<String>,         // sectionBkgColor
    pub section_bkg_color2: Option<String>,        // sectionBkgColor2
    pub sequence_number_color: Option<String>,     // sequenceNumberColor
    pub signal_color: Option<String>,              // signalColor
    pub signal_text_color: Option<String>,         // signalTextColor
    pub special_state_color: Option<String>,       // specialStateColor
    pub state_bkg: Option<String>,                 // stateBkg
    pub state_border: Option<String>,              // stateBorder
    pub state_label_color: Option<String>,         // stateLabelColor
    pub stroke_width: Option<i64>,                 // strokeWidth

    pub surface0: Option<String>,      // surface0
    pub surface1: Option<String>,      // surface1
    pub surface2: Option<String>,      // surface2
    pub surface3: Option<String>,      // surface3
    pub surface4: Option<String>,      // surface4
    pub surface_peer0: Option<String>, // surfacePeer0
    pub surface_peer1: Option<String>, // surfacePeer1
    pub surface_peer2: Option<String>, // surfacePeer2
    pub surface_peer3: Option<String>, // surfacePeer3
    pub surface_peer4: Option<String>, // surfacePeer4

    pub tag_label_background: Option<String>, // tagLabelBackground
    pub tag_label_border: Option<String>,     // tagLabelBorder
    pub tag_label_color: Option<String>,      // tagLabelColor
    pub tag_label_font_size: Option<String>,  // tagLabelFontSize
    pub task_bkg_color: Option<String>,       // taskBkgColor
    pub task_border_color: Option<String>,    // taskBorderColor
    pub task_text_clickable_color: Option<String>, // taskTextClickableColor
    pub task_text_color: Option<String>,      // taskTextColor
    pub task_text_dark_color: Option<String>, // taskTextDarkColor
    pub task_text_light_color: Option<String>, // taskTextLightColor
    pub task_text_outside_color: Option<String>, // taskTextOutsideColor
    pub tertiary_border_color: Option<String>, // tertiaryBorderColor
    pub tertiary_color: Option<String>,       // tertiaryColor
    pub tertiary_text_color: Option<String>,  // tertiaryTextColor
    pub text: Option<String>,                 // text
    pub text_color: Option<String>,           // textColor
    pub title_color: Option<String>,          // titleColor
    pub today_line_color: Option<String>,     // todayLineColor
    pub transition_color: Option<String>,     // transitionColor
    pub transition_label_color: Option<String>, // transitionLabelColor
    pub use_gradient: Option<bool>,           // useGradient

    pub venn1: Option<String>,                 // venn1
    pub venn2: Option<String>,                 // venn2
    pub venn3: Option<String>,                 // venn3
    pub venn4: Option<String>,                 // venn4
    pub venn5: Option<String>,                 // venn5
    pub venn6: Option<String>,                 // venn6
    pub venn7: Option<String>,                 // venn7
    pub venn8: Option<String>,                 // venn8
    pub venn_set_text_color: Option<String>,   // vennSetTextColor
    pub venn_title_text_color: Option<String>, // vennTitleTextColor
    pub vert_line_color: Option<String>,       // vertLineColor

    // Nested groups.
    pub packet: Option<PacketVars>,
    pub radar: Option<RadarVars>,
    pub xy_chart: Option<XyChartVars>,
}

impl ThemeVariables {
    /// Create an empty override (every field `None`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Overlay `overrides` on top of `self` — any `Some(_)` field in
    /// `overrides` replaces the corresponding field in `self`, leaving
    /// `None` fields untouched. Matches upstream's `calculate(overrides)`
    /// semantics where user-provided keys win.
    ///
    /// Nested groups are merged field-by-field so partial overrides of
    /// `radar` / `xyChart` / `packet` don't erase the baseline.
    pub fn merge(&mut self, overrides: ThemeVariables) {
        macro_rules! overlay {
            ($($field:ident),* $(,)?) => {
                $(
                    if overrides.$field.is_some() {
                        self.$field = overrides.$field;
                    }
                )*
            };
        }
        overlay!(
            theme_color_limit,
            activation_bkg_color,
            activation_border_color,
            active_task_bkg_color,
            active_task_border_color,
            actor_bkg,
            actor_border,
            actor_line_color,
            actor_text_color,
            alt_background,
            alt_section_bkg_color,
            arch_edge_arrow_color,
            arch_edge_color,
            arch_edge_width,
            arch_group_border_color,
            arch_group_border_width,
            arrowhead_color,
            attribute_background_color_even,
            attribute_background_color_odd,
            background,
            border1,
            border2,
            branch_label_color,
            c_scale0,
            c_scale1,
            c_scale2,
            c_scale3,
            c_scale4,
            c_scale5,
            c_scale6,
            c_scale7,
            c_scale8,
            c_scale9,
            c_scale10,
            c_scale11,
            c_scale12,
            c_scale_inv0,
            c_scale_inv1,
            c_scale_inv2,
            c_scale_inv3,
            c_scale_inv4,
            c_scale_inv5,
            c_scale_inv6,
            c_scale_inv7,
            c_scale_inv8,
            c_scale_inv9,
            c_scale_inv10,
            c_scale_inv11,
            c_scale_label0,
            c_scale_label1,
            c_scale_label2,
            c_scale_label3,
            c_scale_label4,
            c_scale_label5,
            c_scale_label6,
            c_scale_label7,
            c_scale_label8,
            c_scale_label9,
            c_scale_label10,
            c_scale_label11,
            c_scale_peer0,
            c_scale_peer1,
            c_scale_peer2,
            c_scale_peer3,
            c_scale_peer4,
            c_scale_peer5,
            c_scale_peer6,
            c_scale_peer7,
            c_scale_peer8,
            c_scale_peer9,
            c_scale_peer10,
            c_scale_peer11,
            class_text,
            cluster_bkg,
            cluster_border,
            commit_label_background,
            commit_label_color,
            commit_label_font_size,
            composite_background,
            composite_border,
            composite_title_background,
            contrast,
            crit_bkg_color,
            crit_border_color,
            critical,
            dark_text_color,
            default_link_color,
            done,
            done_task_bkg_color,
            done_task_border_color,
            drop_shadow,
            edge_label_background,
            error_bkg_color,
            error_text_color,
            exclude_bkg_color,
            fill_type0,
            fill_type1,
            fill_type2,
            fill_type3,
            fill_type4,
            fill_type5,
            fill_type6,
            fill_type7,
            font_family,
            font_size,
            font_weight,
            git0,
            git1,
            git2,
            git3,
            git4,
            git5,
            git6,
            git7,
            git_branch_label0,
            git_branch_label1,
            git_branch_label2,
            git_branch_label3,
            git_branch_label4,
            git_branch_label5,
            git_branch_label6,
            git_branch_label7,
            git_inv0,
            git_inv1,
            git_inv2,
            git_inv3,
            git_inv4,
            git_inv5,
            git_inv6,
            git_inv7,
            gradient_start,
            gradient_stop,
            grid_color,
            inner_end_background,
            label_background,
            label_background_color,
            label_box_bkg_color,
            label_box_border_color,
            label_color,
            label_text_color,
            line_color,
            loop_text_color,
            main_bkg,
            main_contrast_color,
            node_bkg,
            node_border,
            node_text_color,
            note,
            note_bkg_color,
            note_border_color,
            note_font_weight,
            note_text_color,
            person_bkg,
            person_border,
            pie0,
            pie1,
            pie2,
            pie3,
            pie4,
            pie5,
            pie6,
            pie7,
            pie8,
            pie9,
            pie10,
            pie11,
            pie12,
            pie_legend_text_color,
            pie_legend_text_size,
            pie_opacity,
            pie_outer_stroke_color,
            pie_outer_stroke_width,
            pie_section_text_color,
            pie_section_text_size,
            pie_stroke_color,
            pie_stroke_width,
            pie_title_text_color,
            pie_title_text_size,
            primary_border_color,
            primary_color,
            primary_text_color,
            quadrant1_fill,
            quadrant1_text_fill,
            quadrant2_fill,
            quadrant2_text_fill,
            quadrant3_fill,
            quadrant3_text_fill,
            quadrant4_fill,
            quadrant4_text_fill,
            quadrant_external_border_stroke_fill,
            quadrant_internal_border_stroke_fill,
            quadrant_point_fill,
            quadrant_point_text_fill,
            quadrant_title_fill,
            quadrant_x_axis_text_fill,
            quadrant_y_axis_text_fill,
            radius,
            relation_color,
            relation_label_background,
            relation_label_color,
            requirement_background,
            requirement_border_color,
            requirement_border_size,
            requirement_text_color,
            row_even,
            row_odd,
            scale_label_color,
            second_bkg,
            secondary_border_color,
            secondary_color,
            secondary_text_color,
            section_bkg_color,
            section_bkg_color2,
            sequence_number_color,
            signal_color,
            signal_text_color,
            special_state_color,
            state_bkg,
            state_border,
            state_label_color,
            stroke_width,
            surface0,
            surface1,
            surface2,
            surface3,
            surface4,
            surface_peer0,
            surface_peer1,
            surface_peer2,
            surface_peer3,
            surface_peer4,
            tag_label_background,
            tag_label_border,
            tag_label_color,
            tag_label_font_size,
            task_bkg_color,
            task_border_color,
            task_text_clickable_color,
            task_text_color,
            task_text_dark_color,
            task_text_light_color,
            task_text_outside_color,
            tertiary_border_color,
            tertiary_color,
            tertiary_text_color,
            text,
            text_color,
            title_color,
            today_line_color,
            transition_color,
            transition_label_color,
            use_gradient,
            venn1,
            venn2,
            venn3,
            venn4,
            venn5,
            venn6,
            venn7,
            venn8,
            venn_set_text_color,
            venn_title_text_color,
            vert_line_color,
        );

        // Nested groups merge per-field so partial radar / xychart / packet
        // overrides don't wipe out the inherited baseline.
        if let Some(src) = overrides.packet {
            let dst = self.packet.get_or_insert_with(PacketVars::default);
            if src.block_fill_color.is_some() {
                dst.block_fill_color = src.block_fill_color;
            }
            if src.block_stroke_color.is_some() {
                dst.block_stroke_color = src.block_stroke_color;
            }
            if src.end_byte_color.is_some() {
                dst.end_byte_color = src.end_byte_color;
            }
            if src.label_color.is_some() {
                dst.label_color = src.label_color;
            }
            if src.start_byte_color.is_some() {
                dst.start_byte_color = src.start_byte_color;
            }
            if src.title_color.is_some() {
                dst.title_color = src.title_color;
            }
        }
        if let Some(src) = overrides.radar {
            let dst = self.radar.get_or_insert_with(RadarVars::default);
            if src.axis_color.is_some() {
                dst.axis_color = src.axis_color;
            }
            if src.axis_label_font_size.is_some() {
                dst.axis_label_font_size = src.axis_label_font_size;
            }
            if src.axis_stroke_width.is_some() {
                dst.axis_stroke_width = src.axis_stroke_width;
            }
            if src.curve_opacity.is_some() {
                dst.curve_opacity = src.curve_opacity;
            }
            if src.curve_stroke_width.is_some() {
                dst.curve_stroke_width = src.curve_stroke_width;
            }
            if src.graticule_color.is_some() {
                dst.graticule_color = src.graticule_color;
            }
            if src.graticule_opacity.is_some() {
                dst.graticule_opacity = src.graticule_opacity;
            }
            if src.graticule_stroke_width.is_some() {
                dst.graticule_stroke_width = src.graticule_stroke_width;
            }
            if src.legend_box_size.is_some() {
                dst.legend_box_size = src.legend_box_size;
            }
            if src.legend_font_size.is_some() {
                dst.legend_font_size = src.legend_font_size;
            }
        }
        if let Some(src) = overrides.xy_chart {
            let dst = self.xy_chart.get_or_insert_with(XyChartVars::default);
            if src.background_color.is_some() {
                dst.background_color = src.background_color;
            }
            if src.data_label_color.is_some() {
                dst.data_label_color = src.data_label_color;
            }
            if src.plot_color_palette.is_some() {
                dst.plot_color_palette = src.plot_color_palette;
            }
            if src.title_color.is_some() {
                dst.title_color = src.title_color;
            }
            if src.x_axis_label_color.is_some() {
                dst.x_axis_label_color = src.x_axis_label_color;
            }
            if src.x_axis_line_color.is_some() {
                dst.x_axis_line_color = src.x_axis_line_color;
            }
            if src.x_axis_tick_color.is_some() {
                dst.x_axis_tick_color = src.x_axis_tick_color;
            }
            if src.x_axis_title_color.is_some() {
                dst.x_axis_title_color = src.x_axis_title_color;
            }
            if src.y_axis_label_color.is_some() {
                dst.y_axis_label_color = src.y_axis_label_color;
            }
            if src.y_axis_line_color.is_some() {
                dst.y_axis_line_color = src.y_axis_line_color;
            }
            if src.y_axis_tick_color.is_some() {
                dst.y_axis_tick_color = src.y_axis_tick_color;
            }
            if src.y_axis_title_color.is_some() {
                dst.y_axis_title_color = src.y_axis_title_color;
            }
        }
    }
}

/// Resolve a theme variant by its upstream name. Unknown names fall
/// back to `default`, matching upstream's behaviour when an unknown
/// `theme` string reaches the renderer.
///
/// Accepted keys (case-sensitive, lowercase): `"default"`, `"base"`,
/// `"dark"`, `"forest"`, `"neutral"`. An empty string is treated as
/// `default` for convenience when a config omits the field.
#[must_use]
pub fn get_theme(name: &str) -> ThemeVariables {
    match name {
        "default" | "" => default::variables(),
        "base" => base::variables(),
        "dark" => dark::variables(),
        "forest" => forest::variables(),
        "neutral" => neutral::variables(),
        _ => default::variables(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_theme_dispatches_to_each_variant() {
        assert_eq!(
            get_theme("default").primary_color.as_deref(),
            Some("#ECECFF")
        );
        assert_eq!(get_theme("").primary_color.as_deref(), Some("#ECECFF"));
        assert_eq!(get_theme("base").primary_color.as_deref(), Some("#fff4dd"));
        assert_eq!(get_theme("dark").primary_color.as_deref(), Some("#1f2020"));
        assert_eq!(
            get_theme("forest").primary_color.as_deref(),
            Some("#cde498")
        );
        assert_eq!(get_theme("neutral").primary_color.as_deref(), Some("#eee"));
        // Unknown name falls through to default.
        assert_eq!(
            get_theme("__not_a_theme__").primary_color.as_deref(),
            Some("#ECECFF")
        );
    }

    #[test]
    fn merge_overlays_user_supplied_fields() {
        let mut base = default::variables();
        let mut overrides = ThemeVariables::new();
        overrides.primary_color = Some("#123456".into());
        overrides.radar = Some(RadarVars {
            axis_color: Some("#abcdef".into()),
            ..Default::default()
        });
        base.merge(overrides);
        assert_eq!(base.primary_color.as_deref(), Some("#123456"));
        // Sibling fields preserved.
        assert_eq!(base.background.as_deref(), Some("white"));
        // Radar merged per-field (not replaced wholesale).
        let radar = base.radar.expect("radar present");
        assert_eq!(radar.axis_color.as_deref(), Some("#abcdef"));
        assert_eq!(radar.axis_label_font_size, Some(12));
    }
}
