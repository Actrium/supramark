//! Mermaid's stock `default` theme — the one you get when no
//! `theme` key is set. Light background, pastel primary colours.
//!
//! Seed values and derived colors mirror
//! `packages/mermaid/src/themes/theme-default.js` from upstream
//! mermaid@11.14.0. Derived fields were resolved ahead of time by
//! importing the upstream JS under a node shim and calling
//! `getThemeVariables()`; the resulting flat map is copied here as
//! literal constants. This keeps Wave 0 free of runtime color math.

use super::{RadarVars, ThemeVariables, XyChartVars};

#[allow(clippy::field_reassign_with_default, clippy::needless_update)]
/// Return a fully-populated [`ThemeVariables`] for the `default` theme.
#[must_use]
pub fn variables() -> ThemeVariables {
    let mut v = ThemeVariables::default();
    v.theme_color_limit = Some(12_i64);
    v.activation_bkg_color = Some("#f4f4f4".into());
    v.activation_border_color = Some("#666".into());
    v.active_task_bkg_color = Some("#bfc7ff".into());
    v.active_task_border_color = Some("#534fbc".into());
    v.actor_bkg = Some("#ECECFF".into());
    v.actor_border = Some("#9370DB".into());
    v.actor_line_color = Some("#9370DB".into());
    v.actor_text_color = Some("black".into());
    v.alt_background = Some("#f0f0f0".into());
    v.alt_section_bkg_color = Some("white".into());
    v.arch_edge_arrow_color = Some("#333333".into());
    v.arch_edge_color = Some("#333333".into());
    v.arch_edge_width = Some("3".into());
    v.arch_group_border_color = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.arch_group_border_width = Some("2px".into());
    v.arrowhead_color = Some("#333333".into());
    v.attribute_background_color_even = Some("#f2f2f2".into());
    v.attribute_background_color_odd = Some("#ffffff".into());
    v.background = Some("white".into());
    v.border1 = Some("#9370DB".into());
    v.border2 = Some("#aaaa33".into());
    v.c_scale0 = Some("hsl(240, 100%, 76.2745098039%)".into());
    v.c_scale1 = Some("hsl(60, 100%, 73.5294117647%)".into());
    v.c_scale10 = Some("hsl(180, 100%, 76.2745098039%)".into());
    v.c_scale11 = Some("hsl(210, 100%, 76.2745098039%)".into());
    v.c_scale2 = Some("hsl(80, 100%, 76.2745098039%)".into());
    v.c_scale3 = Some("hsl(270, 100%, 76.2745098039%)".into());
    v.c_scale4 = Some("hsl(300, 100%, 76.2745098039%)".into());
    v.c_scale5 = Some("hsl(330, 100%, 76.2745098039%)".into());
    v.c_scale6 = Some("hsl(0, 100%, 76.2745098039%)".into());
    v.c_scale7 = Some("hsl(30, 100%, 76.2745098039%)".into());
    v.c_scale8 = Some("hsl(90, 100%, 76.2745098039%)".into());
    v.c_scale9 = Some("hsl(150, 100%, 76.2745098039%)".into());
    v.c_scale_inv0 = Some("hsl(60, 100%, 86.2745098039%)".into());
    v.c_scale_inv1 = Some("hsl(240, 100%, 83.5294117647%)".into());
    v.c_scale_inv10 = Some("hsl(0, 100%, 86.2745098039%)".into());
    v.c_scale_inv11 = Some("hsl(30, 100%, 86.2745098039%)".into());
    v.c_scale_inv2 = Some("hsl(260, 100%, 86.2745098039%)".into());
    v.c_scale_inv3 = Some("hsl(90, 100%, 86.2745098039%)".into());
    v.c_scale_inv4 = Some("hsl(120, 100%, 86.2745098039%)".into());
    v.c_scale_inv5 = Some("hsl(150, 100%, 86.2745098039%)".into());
    v.c_scale_inv6 = Some("hsl(180, 100%, 86.2745098039%)".into());
    v.c_scale_inv7 = Some("hsl(210, 100%, 86.2745098039%)".into());
    v.c_scale_inv8 = Some("hsl(270, 100%, 86.2745098039%)".into());
    v.c_scale_inv9 = Some("hsl(330, 100%, 86.2745098039%)".into());
    v.c_scale_label0 = Some("#ffffff".into());
    v.c_scale_label1 = Some("black".into());
    v.c_scale_label10 = Some("black".into());
    v.c_scale_label11 = Some("black".into());
    v.c_scale_label2 = Some("black".into());
    v.c_scale_label3 = Some("#ffffff".into());
    v.c_scale_label4 = Some("black".into());
    v.c_scale_label5 = Some("black".into());
    v.c_scale_label6 = Some("black".into());
    v.c_scale_label7 = Some("black".into());
    v.c_scale_label8 = Some("black".into());
    v.c_scale_label9 = Some("black".into());
    v.c_scale_peer0 = Some("hsl(240, 100%, 61.2745098039%)".into());
    v.c_scale_peer1 = Some("hsl(60, 100%, 48.5294117647%)".into());
    v.c_scale_peer10 = Some("hsl(180, 100%, 61.2745098039%)".into());
    v.c_scale_peer11 = Some("hsl(210, 100%, 61.2745098039%)".into());
    v.c_scale_peer2 = Some("hsl(80, 100%, 56.2745098039%)".into());
    v.c_scale_peer3 = Some("hsl(270, 100%, 61.2745098039%)".into());
    v.c_scale_peer4 = Some("hsl(300, 100%, 61.2745098039%)".into());
    v.c_scale_peer5 = Some("hsl(330, 100%, 61.2745098039%)".into());
    v.c_scale_peer6 = Some("hsl(0, 100%, 61.2745098039%)".into());
    v.c_scale_peer7 = Some("hsl(30, 100%, 61.2745098039%)".into());
    v.c_scale_peer8 = Some("hsl(90, 100%, 61.2745098039%)".into());
    v.c_scale_peer9 = Some("hsl(150, 100%, 61.2745098039%)".into());
    v.class_text = Some("#131300".into());
    v.cluster_bkg = Some("#ffffde".into());
    v.cluster_border = Some("#aaaa33".into());
    v.commit_label_background = Some("#ffffde".into());
    v.commit_label_color = Some("#000021".into());
    v.commit_label_font_size = Some("10px".into());
    v.composite_background = Some("white".into());
    v.composite_border = Some("#9370DB".into());
    v.composite_title_background = Some("#ECECFF".into());
    v.crit_bkg_color = Some("red".into());
    v.crit_border_color = Some("#ff8888".into());
    v.default_link_color = Some("#333333".into());
    v.done_task_bkg_color = Some("lightgrey".into());
    v.done_task_border_color = Some("grey".into());
    v.drop_shadow = Some("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))".into());
    v.edge_label_background = Some("rgba(232,232,232, 0.8)".into());
    v.error_bkg_color = Some("#552222".into());
    v.error_text_color = Some("#552222".into());
    v.exclude_bkg_color = Some("#eeeeee".into());
    v.fill_type0 = Some("#ECECFF".into());
    v.fill_type1 = Some("#ffffde".into());
    v.fill_type2 = Some("hsl(304, 100%, 96.2745098039%)".into());
    v.fill_type3 = Some("hsl(124, 100%, 93.5294117647%)".into());
    v.fill_type4 = Some("hsl(176, 100%, 96.2745098039%)".into());
    v.fill_type5 = Some("hsl(-4, 100%, 93.5294117647%)".into());
    v.fill_type6 = Some("hsl(8, 100%, 96.2745098039%)".into());
    v.fill_type7 = Some("hsl(188, 100%, 93.5294117647%)".into());
    v.font_family = Some("\"trebuchet ms\", verdana, arial, sans-serif".into());
    v.font_size = Some("16px".into());
    v.font_weight = Some("normal".into());
    v.git0 = Some("hsl(240, 100%, 46.2745098039%)".into());
    v.git1 = Some("hsl(60, 100%, 43.5294117647%)".into());
    v.git2 = Some("hsl(80, 100%, 46.2745098039%)".into());
    v.git3 = Some("hsl(210, 100%, 46.2745098039%)".into());
    v.git4 = Some("hsl(180, 100%, 46.2745098039%)".into());
    v.git5 = Some("hsl(150, 100%, 46.2745098039%)".into());
    v.git6 = Some("hsl(300, 100%, 46.2745098039%)".into());
    v.git7 = Some("hsl(0, 100%, 46.2745098039%)".into());
    v.git_branch_label0 = Some("#ffffff".into());
    v.git_branch_label1 = Some("black".into());
    v.git_branch_label2 = Some("black".into());
    v.git_branch_label3 = Some("#ffffff".into());
    v.git_branch_label4 = Some("black".into());
    v.git_branch_label5 = Some("black".into());
    v.git_branch_label6 = Some("black".into());
    v.git_branch_label7 = Some("black".into());
    v.git_inv0 = Some("hsl(60, 100%, 3.7254901961%)".into());
    v.git_inv1 = Some("rgb(0, 0, 160.5)".into());
    v.git_inv2 = Some("rgb(48.8333333334, 0, 146.5000000001)".into());
    v.git_inv3 = Some("rgb(146.5000000001, 73.2500000001, 0)".into());
    v.git_inv4 = Some("rgb(146.5000000001, 0, 0)".into());
    v.git_inv5 = Some("rgb(146.5000000001, 0, 73.2500000001)".into());
    v.git_inv6 = Some("rgb(0, 146.5000000001, 0)".into());
    v.git_inv7 = Some("rgb(0, 146.5000000001, 146.5000000001)".into());
    v.gradient_start = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.gradient_stop = Some("hsl(60, 60%, 83.5294117647%)".into());
    v.grid_color = Some("lightgrey".into());
    v.inner_end_background = Some("#9370DB".into());
    v.label_background = Some("rgba(232,232,232, 0.8)".into());
    v.label_background_color = Some("#ECECFF".into());
    v.label_box_bkg_color = Some("#ECECFF".into());
    v.label_box_border_color = Some("#9370DB".into());
    v.label_color = Some("black".into());
    v.label_text_color = Some("black".into());
    v.line_color = Some("#333333".into());
    v.loop_text_color = Some("black".into());
    v.main_bkg = Some("#ECECFF".into());
    v.node_bkg = Some("#ECECFF".into());
    v.node_border = Some("#9370DB".into());
    v.note_bkg_color = Some("#fff5ad".into());
    v.note_border_color = Some("#aaaa33".into());
    v.note_font_weight = Some("normal".into());
    v.note_text_color = Some("black".into());
    v.person_bkg = Some("#ECECFF".into());
    v.person_border = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.pie1 = Some("#ECECFF".into());
    v.pie10 = Some("hsl(300, 100%, 56.2745098039%)".into());
    v.pie11 = Some("hsl(150, 100%, 56.2745098039%)".into());
    v.pie12 = Some("hsl(0, 100%, 66.2745098039%)".into());
    v.pie2 = Some("#ffffde".into());
    v.pie3 = Some("hsl(80, 100%, 56.2745098039%)".into());
    v.pie4 = Some("hsl(240, 100%, 86.2745098039%)".into());
    v.pie5 = Some("hsl(60, 100%, 63.5294117647%)".into());
    v.pie6 = Some("hsl(80, 100%, 76.2745098039%)".into());
    v.pie7 = Some("hsl(300, 100%, 76.2745098039%)".into());
    v.pie8 = Some("hsl(180, 100%, 56.2745098039%)".into());
    v.pie9 = Some("hsl(0, 100%, 56.2745098039%)".into());
    v.pie_legend_text_color = Some("black".into());
    v.pie_legend_text_size = Some("17px".into());
    v.pie_opacity = Some("0.7".into());
    v.pie_outer_stroke_color = Some("black".into());
    v.pie_outer_stroke_width = Some("2px".into());
    v.pie_section_text_color = Some("#333".into());
    v.pie_section_text_size = Some("17px".into());
    v.pie_stroke_color = Some("black".into());
    v.pie_stroke_width = Some("2px".into());
    v.pie_title_text_color = Some("black".into());
    v.pie_title_text_size = Some("25px".into());
    v.primary_border_color = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.primary_color = Some("#ECECFF".into());
    v.primary_text_color = Some("#131300".into());
    v.quadrant1_fill = Some("#ECECFF".into());
    v.quadrant1_text_fill = Some("#131300".into());
    v.quadrant2_fill = Some("#f1f1ff".into());
    v.quadrant2_text_fill = Some("#0e0e00".into());
    v.quadrant3_fill = Some("#f6f6ff".into());
    v.quadrant3_text_fill = Some("#090900".into());
    v.quadrant4_fill = Some("#fbfbff".into());
    v.quadrant4_text_fill = Some("#040400".into());
    v.quadrant_external_border_stroke_fill = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.quadrant_internal_border_stroke_fill = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.quadrant_point_fill = Some("hsl(240, 100%, NaN%)".into());
    v.quadrant_point_text_fill = Some("#131300".into());
    v.quadrant_title_fill = Some("#131300".into());
    v.quadrant_x_axis_text_fill = Some("#131300".into());
    v.quadrant_y_axis_text_fill = Some("#131300".into());
    v.radius = Some(5_i64);
    v.relation_color = Some("#333333".into());
    v.relation_label_background = Some("rgba(232,232,232, 0.8)".into());
    v.relation_label_color = Some("black".into());
    v.requirement_background = Some("#ECECFF".into());
    v.requirement_border_color = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.requirement_border_size = Some("1".into());
    v.requirement_text_color = Some("#131300".into());
    v.row_even = Some("hsl(240, 100%, 97.2745098039%)".into());
    v.row_odd = Some("hsl(240, 100%, 100%)".into());
    v.scale_label_color = Some("black".into());
    v.second_bkg = Some("#ffffde".into());
    v.secondary_border_color = Some("hsl(60, 60%, 83.5294117647%)".into());
    v.secondary_color = Some("#ffffde".into());
    v.secondary_text_color = Some("#000021".into());
    v.section_bkg_color = Some("rgba(102, 102, 255, 0.49)".into());
    v.section_bkg_color2 = Some("#fff400".into());
    v.sequence_number_color = Some("white".into());
    v.signal_color = Some("#333".into());
    v.signal_text_color = Some("#333".into());
    v.special_state_color = Some("#333333".into());
    v.state_bkg = Some("#ECECFF".into());
    v.state_label_color = Some("#131300".into());
    v.stroke_width = Some(1_i64);
    v.surface0 = Some("hsl(270, 100%, 91.2745098039%)".into());
    v.surface1 = Some("hsl(270, 100%, 86.2745098039%)".into());
    v.surface2 = Some("hsl(270, 100%, 81.2745098039%)".into());
    v.surface3 = Some("hsl(270, 100%, 76.2745098039%)".into());
    v.surface4 = Some("hsl(270, 100%, 71.2745098039%)".into());
    v.surface_peer0 = Some("hsl(270, 100%, 89.2745098039%)".into());
    v.surface_peer1 = Some("hsl(270, 100%, 84.2745098039%)".into());
    v.surface_peer2 = Some("hsl(270, 100%, 79.2745098039%)".into());
    v.surface_peer3 = Some("hsl(270, 100%, 74.2745098039%)".into());
    v.surface_peer4 = Some("hsl(270, 100%, 69.2745098039%)".into());
    v.tag_label_background = Some("#ECECFF".into());
    v.tag_label_border = Some("hsl(240, 60%, 86.2745098039%)".into());
    v.tag_label_color = Some("#131300".into());
    v.tag_label_font_size = Some("10px".into());
    v.task_bkg_color = Some("#8a90dd".into());
    v.task_border_color = Some("#534fbc".into());
    v.task_text_clickable_color = Some("#003163".into());
    v.task_text_color = Some("white".into());
    v.task_text_dark_color = Some("black".into());
    v.task_text_light_color = Some("white".into());
    v.task_text_outside_color = Some("black".into());
    v.tertiary_border_color = Some("hsl(80, 60%, 86.2745098039%)".into());
    v.tertiary_color = Some("hsl(80, 100%, 96.2745098039%)".into());
    v.tertiary_text_color = Some("rgb(6.3333333334, 0, 19.0000000001)".into());
    v.text_color = Some("#333".into());
    v.title_color = Some("#333".into());
    v.today_line_color = Some("red".into());
    v.transition_color = Some("#333333".into());
    v.transition_label_color = Some("#333".into());
    v.use_gradient = Some(false);
    v.venn1 = Some("hsl(240, 100%, 66.2745098039%)".into());
    v.venn2 = Some("hsl(60, 100%, 63.5294117647%)".into());
    v.venn3 = Some("hsl(80, 100%, 56.2745098039%)".into());
    v.venn4 = Some("hsl(300, 100%, 66.2745098039%)".into());
    v.venn5 = Some("hsl(180, 100%, 66.2745098039%)".into());
    v.venn6 = Some("hsl(120, 100%, 63.5294117647%)".into());
    v.venn7 = Some("hsl(0, 100%, 66.2745098039%)".into());
    v.venn8 = Some("hsl(180, 100%, 63.5294117647%)".into());
    v.venn_set_text_color = Some("#333".into());
    v.venn_title_text_color = Some("#333".into());
    v.vert_line_color = Some("navy".into());
    v.radar = Some(RadarVars {
        axis_color: Some("#333333".into()),
        axis_label_font_size: Some(12_i64),
        axis_stroke_width: Some(2_i64),
        curve_opacity: Some(0.5_f64),
        curve_stroke_width: Some(2_i64),
        graticule_color: Some("#DEDEDE".into()),
        graticule_opacity: Some(0.3_f64),
        graticule_stroke_width: Some(1_i64),
        legend_box_size: Some(12_i64),
        legend_font_size: Some(12_i64),
        ..Default::default()
    });
    v.xy_chart = Some(XyChartVars {
        background_color: Some("white".into()),
        data_label_color: Some("#131300".into()),
        plot_color_palette: Some("#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3".into()),
        title_color: Some("#131300".into()),
        x_axis_label_color: Some("#131300".into()),
        x_axis_line_color: Some("#131300".into()),
        x_axis_tick_color: Some("#131300".into()),
        x_axis_title_color: Some("#131300".into()),
        y_axis_label_color: Some("#131300".into()),
        y_axis_line_color: Some("#131300".into()),
        y_axis_tick_color: Some("#131300".into()),
        y_axis_title_color: Some("#131300".into()),
        ..Default::default()
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_color_matches_upstream() {
        let v = variables();
        assert_eq!(v.primary_color.as_deref(), Some("#ECECFF"));
    }

    #[test]
    fn background_matches_upstream() {
        let v = variables();
        assert_eq!(v.background.as_deref(), Some("white"));
    }

    #[test]
    fn font_family_is_trebuchet() {
        let v = variables();
        assert_eq!(
            v.font_family.as_deref(),
            Some("\"trebuchet ms\", verdana, arial, sans-serif")
        );
    }
}
