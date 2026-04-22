//! Mermaid's `base` theme — the minimal seed theme designed to
//! be used as a starting point for user overrides. All derived
//! colours are produced from a single `primaryColor`.
//!
//! Seed values and derived colors mirror
//! `packages/mermaid/src/themes/theme-base.js` from upstream
//! mermaid@11.14.0. Derived fields were resolved ahead of time by
//! importing the upstream JS under a node shim and calling
//! `getThemeVariables()`; the resulting flat map is copied here as
//! literal constants. This keeps Wave 0 free of runtime color math.

use super::{RadarVars, ThemeVariables, XyChartVars};

#[allow(clippy::field_reassign_with_default, clippy::needless_update)]
/// Return a fully-populated [`ThemeVariables`] for the `base` theme.
#[must_use]
pub fn variables() -> ThemeVariables {
    let mut v = ThemeVariables::default();
    v.theme_color_limit = Some(12_i64);
    v.activation_bkg_color = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.activation_border_color = Some("hsl(-79.4117647059, 100%, 83.3333333333%)".into());
    v.active_task_bkg_color = Some("hsl(40.5882352941, 100%, 100%)".into());
    v.active_task_border_color = Some("#fff4dd".into());
    v.actor_bkg = Some("#fff4dd".into());
    v.actor_border = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.actor_line_color = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.actor_text_color = Some("#333".into());
    v.alt_background = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.alt_section_bkg_color = Some("white".into());
    v.arch_edge_arrow_color = Some("#777".into());
    v.arch_edge_color = Some("#777".into());
    v.arch_edge_width = Some("3".into());
    v.arch_group_border_color = Some("#000".into());
    v.arch_group_border_width = Some("2px".into());
    v.arrowhead_color = Some("#0b0b0b".into());
    v.attribute_background_color_even = Some("#f2f2f2".into());
    v.attribute_background_color_odd = Some("#ffffff".into());
    v.background = Some("#f4f4f4".into());
    v.border2 = Some("hsl(220.5882352941, 60%, 88.3333333333%)".into());
    v.branch_label_color = Some("#333".into());
    v.c_scale0 = Some("hsl(40.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale1 = Some("hsl(-79.4117647059, 100%, 68.3333333333%)".into());
    v.c_scale10 = Some("hsl(340.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale11 = Some("hsl(10.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale2 = Some("hsl(220.5882352941, 100%, 73.3333333333%)".into());
    v.c_scale3 = Some("hsl(70.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale4 = Some("hsl(100.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale5 = Some("hsl(130.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale6 = Some("hsl(160.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale7 = Some("hsl(190.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale8 = Some("hsl(250.5882352941, 100%, 75%)".into());
    v.c_scale9 = Some("hsl(310.5882352941, 100%, 68.3333333333%)".into());
    v.c_scale_inv0 = Some("rgb(0, 52.2500000001, 161.5000000002)".into());
    v.c_scale_inv1 = Some("rgb(52.2500000001, 161.5000000002, 0)".into());
    v.c_scale_inv10 = Some("rgb(0, 161.5000000002, 109.2500000001)".into());
    v.c_scale_inv11 = Some("rgb(0, 133.0000000002, 161.5000000002)".into());
    v.c_scale_inv2 = Some("rgb(136.0000000002, 92.0000000001, 0)".into());
    v.c_scale_inv3 = Some("rgb(28.5, 0, 161.5000000002)".into());
    v.c_scale_inv4 = Some("rgb(109.2500000001, 0, 161.5000000002)".into());
    v.c_scale_inv5 = Some("rgb(161.5000000002, 0, 133.0000000002)".into());
    v.c_scale_inv6 = Some("rgb(161.5000000002, 0, 52.2500000001)".into());
    v.c_scale_inv7 = Some("rgb(161.5000000002, 28.5, 0)".into());
    v.c_scale_inv8 = Some("rgb(105, 127.5, 0)".into());
    v.c_scale_inv9 = Some("rgb(0, 161.5000000002, 28.5)".into());
    v.c_scale_label0 = Some("#333".into());
    v.c_scale_label1 = Some("#333".into());
    v.c_scale_label10 = Some("#333".into());
    v.c_scale_label11 = Some("#333".into());
    v.c_scale_label2 = Some("#333".into());
    v.c_scale_label3 = Some("#333".into());
    v.c_scale_label4 = Some("#333".into());
    v.c_scale_label5 = Some("#333".into());
    v.c_scale_label6 = Some("#333".into());
    v.c_scale_label7 = Some("#333".into());
    v.c_scale_label8 = Some("#333".into());
    v.c_scale_label9 = Some("#333".into());
    v.c_scale_peer0 = Some("hsl(40.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer1 = Some("hsl(-79.4117647059, 100%, 58.3333333333%)".into());
    v.c_scale_peer10 = Some("hsl(340.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer11 = Some("hsl(10.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer2 = Some("hsl(220.5882352941, 100%, 63.3333333333%)".into());
    v.c_scale_peer3 = Some("hsl(70.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer4 = Some("hsl(100.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer5 = Some("hsl(130.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer6 = Some("hsl(160.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer7 = Some("hsl(190.5882352941, 100%, 58.3333333333%)".into());
    v.c_scale_peer8 = Some("hsl(250.5882352941, 100%, 65%)".into());
    v.c_scale_peer9 = Some("hsl(310.5882352941, 100%, 58.3333333333%)".into());
    v.class_text = Some("#333".into());
    v.cluster_bkg = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.cluster_border = Some("hsl(220.5882352941, 60%, 88.3333333333%)".into());
    v.commit_label_background = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.commit_label_color = Some("rgb(11.0000000001, 34.0000000002, 0)".into());
    v.commit_label_font_size = Some("10px".into());
    v.composite_background = Some("#f4f4f4".into());
    v.composite_border = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.composite_title_background = Some("#fff4dd".into());
    v.crit_bkg_color = Some("red".into());
    v.crit_border_color = Some("#ff8888".into());
    v.default_link_color = Some("#0b0b0b".into());
    v.done_task_bkg_color = Some("lightgrey".into());
    v.done_task_border_color = Some("grey".into());
    v.drop_shadow = Some("drop-shadow( 1px 2px 2px rgba(185,185,185,1))".into());
    v.edge_label_background = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.error_bkg_color = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.error_text_color = Some("rgb(8.5000000002, 5.7500000001, 0)".into());
    v.exclude_bkg_color = Some("#eeeeee".into());
    v.fill_type0 = Some("#fff4dd".into());
    v.fill_type1 = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.fill_type2 = Some("hsl(104.5882352941, 100%, 93.3333333333%)".into());
    v.fill_type3 = Some("hsl(-15.4117647059, 100%, 93.3333333333%)".into());
    v.fill_type4 = Some("hsl(-23.4117647059, 100%, 93.3333333333%)".into());
    v.fill_type5 = Some("hsl(-143.4117647059, 100%, 93.3333333333%)".into());
    v.fill_type6 = Some("hsl(168.5882352941, 100%, 93.3333333333%)".into());
    v.fill_type7 = Some("hsl(48.5882352941, 100%, 93.3333333333%)".into());
    v.font_family = Some("\"trebuchet ms\", verdana, arial, sans-serif".into());
    v.font_size = Some("16px".into());
    v.font_weight = Some("normal".into());
    v.git0 = Some("hsl(40.5882352941, 100%, 68.3333333333%)".into());
    v.git1 = Some("hsl(-79.4117647059, 100%, 68.3333333333%)".into());
    v.git2 = Some("hsl(220.5882352941, 100%, 73.3333333333%)".into());
    v.git3 = Some("hsl(10.5882352941, 100%, 68.3333333333%)".into());
    v.git4 = Some("hsl(-19.4117647059, 100%, 68.3333333333%)".into());
    v.git5 = Some("hsl(-49.4117647059, 100%, 68.3333333333%)".into());
    v.git6 = Some("hsl(100.5882352941, 100%, 68.3333333333%)".into());
    v.git7 = Some("hsl(160.5882352941, 100%, 68.3333333333%)".into());
    v.git_branch_label0 = Some("#333".into());
    v.git_branch_label1 = Some("#333".into());
    v.git_branch_label2 = Some("#333".into());
    v.git_branch_label3 = Some("#333".into());
    v.git_branch_label4 = Some("#333".into());
    v.git_branch_label5 = Some("#333".into());
    v.git_branch_label6 = Some("#333".into());
    v.git_branch_label7 = Some("#333".into());
    v.git_inv0 = Some("rgb(0, 52.2500000001, 161.5000000002)".into());
    v.git_inv1 = Some("rgb(52.2500000001, 161.5000000002, 0)".into());
    v.git_inv2 = Some("rgb(136.0000000002, 92.0000000001, 0)".into());
    v.git_inv3 = Some("rgb(0, 133.0000000002, 161.5000000002)".into());
    v.git_inv4 = Some("rgb(0, 161.5000000002, 109.2500000001)".into());
    v.git_inv5 = Some("rgb(0, 161.5000000002, 28.5)".into());
    v.git_inv6 = Some("rgb(109.2500000001, 0, 161.5000000002)".into());
    v.git_inv7 = Some("rgb(161.5000000002, 0, 52.2500000001)".into());
    v.gradient_start = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.gradient_stop = Some("hsl(-79.4117647059, 60%, 83.3333333333%)".into());
    v.grid_color = Some("lightgrey".into());
    v.inner_end_background = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.label_background_color = Some("#fff4dd".into());
    v.label_box_bkg_color = Some("#fff4dd".into());
    v.label_box_border_color = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.label_text_color = Some("#333".into());
    v.line_color = Some("#0b0b0b".into());
    v.loop_text_color = Some("#333".into());
    v.main_bkg = Some("#fff4dd".into());
    v.node_bkg = Some("#fff4dd".into());
    v.node_border = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.node_text_color = Some("#333".into());
    v.note_bkg_color = Some("#fff5ad".into());
    v.note_border_color = Some("hsl(52.6829268293, 60%, 73.9215686275%)".into());
    v.note_font_weight = Some("normal".into());
    v.note_text_color = Some("#333".into());
    v.person_bkg = Some("#fff4dd".into());
    v.person_border = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.pie1 = Some("#fff4dd".into());
    v.pie10 = Some("hsl(100.5882352941, 100%, 73.3333333333%)".into());
    v.pie11 = Some("hsl(-19.4117647059, 100%, 73.3333333333%)".into());
    v.pie12 = Some("hsl(160.5882352941, 100%, 83.3333333333%)".into());
    v.pie2 = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.pie3 = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.pie4 = Some("hsl(40.5882352941, 100%, 83.3333333333%)".into());
    v.pie5 = Some("hsl(-79.4117647059, 100%, 83.3333333333%)".into());
    v.pie6 = Some("hsl(220.5882352941, 100%, 88.3333333333%)".into());
    v.pie7 = Some("hsl(100.5882352941, 100%, 83.3333333333%)".into());
    v.pie8 = Some("hsl(-19.4117647059, 100%, 83.3333333333%)".into());
    v.pie9 = Some("hsl(160.5882352941, 100%, 93.3333333333%)".into());
    v.pie_legend_text_color = Some("#333".into());
    v.pie_legend_text_size = Some("17px".into());
    v.pie_opacity = Some("0.7".into());
    v.pie_outer_stroke_color = Some("black".into());
    v.pie_outer_stroke_width = Some("2px".into());
    v.pie_section_text_color = Some("#333".into());
    v.pie_section_text_size = Some("17px".into());
    v.pie_stroke_color = Some("black".into());
    v.pie_stroke_width = Some("2px".into());
    v.pie_title_text_color = Some("#333".into());
    v.pie_title_text_size = Some("25px".into());
    v.primary_border_color = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.primary_color = Some("#fff4dd".into());
    v.primary_text_color = Some("#333".into());
    v.quadrant1_fill = Some("#fff4dd".into());
    v.quadrant1_text_fill = Some("#333".into());
    v.quadrant2_fill = Some("#fff9e2".into());
    v.quadrant2_text_fill = Some("#2e2e2e".into());
    v.quadrant3_fill = Some("#fffee7".into());
    v.quadrant3_text_fill = Some("#292929".into());
    v.quadrant4_fill = Some("#ffffec".into());
    v.quadrant4_text_fill = Some("#242424".into());
    v.quadrant_external_border_stroke_fill = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.quadrant_internal_border_stroke_fill = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.quadrant_point_fill = Some("hsl(40.5882352941, 100%, NaN%)".into());
    v.quadrant_point_text_fill = Some("#333".into());
    v.quadrant_title_fill = Some("#333".into());
    v.quadrant_x_axis_text_fill = Some("#333".into());
    v.quadrant_y_axis_text_fill = Some("#333".into());
    v.radius = Some(5_i64);
    v.relation_color = Some("#0b0b0b".into());
    v.relation_label_background = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.relation_label_color = Some("#333".into());
    v.requirement_background = Some("#fff4dd".into());
    v.requirement_border_color = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.requirement_border_size = Some("1".into());
    v.requirement_text_color = Some("#333".into());
    v.row_even = Some("hsl(40.5882352941, 100%, 98.3333333333%)".into());
    v.row_odd = Some("hsl(40.5882352941, 100%, 100%)".into());
    v.scale_label_color = Some("#333".into());
    v.secondary_border_color = Some("hsl(-79.4117647059, 60%, 83.3333333333%)".into());
    v.secondary_color = Some("hsl(-79.4117647059, 100%, 93.3333333333%)".into());
    v.secondary_text_color = Some("rgb(11.0000000001, 34.0000000002, 0)".into());
    v.section_bkg_color = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.section_bkg_color2 = Some("#fff4dd".into());
    v.sequence_number_color = Some("#f4f4f4".into());
    v.signal_color = Some("#333".into());
    v.signal_text_color = Some("#333".into());
    v.special_state_color = Some("#0b0b0b".into());
    v.state_bkg = Some("#fff4dd".into());
    v.state_label_color = Some("#333".into());
    v.stroke_width = Some(1_i64);
    v.surface0 = Some("hsl(220.5882352941, 85%, 88.3333333333%)".into());
    v.surface1 = Some("hsl(220.5882352941, 85%, 85.3333333333%)".into());
    v.surface2 = Some("hsl(220.5882352941, 85%, 82.3333333333%)".into());
    v.surface3 = Some("hsl(220.5882352941, 85%, 79.3333333333%)".into());
    v.surface4 = Some("hsl(220.5882352941, 85%, 76.3333333333%)".into());
    v.surface_peer0 = Some("hsl(220.5882352941, 85%, 85.3333333333%)".into());
    v.surface_peer1 = Some("hsl(220.5882352941, 85%, 82.3333333333%)".into());
    v.surface_peer2 = Some("hsl(220.5882352941, 85%, 79.3333333333%)".into());
    v.surface_peer3 = Some("hsl(220.5882352941, 85%, 76.3333333333%)".into());
    v.surface_peer4 = Some("hsl(220.5882352941, 85%, 73.3333333333%)".into());
    v.tag_label_background = Some("#fff4dd".into());
    v.tag_label_border = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.tag_label_color = Some("#333".into());
    v.tag_label_font_size = Some("10px".into());
    v.task_bkg_color = Some("#fff4dd".into());
    v.task_border_color = Some("hsl(40.5882352941, 60%, 83.3333333333%)".into());
    v.task_text_clickable_color = Some("#003163".into());
    v.task_text_color = Some("#333".into());
    v.task_text_dark_color = Some("#333".into());
    v.task_text_light_color = Some("#333".into());
    v.task_text_outside_color = Some("#333".into());
    v.tertiary_border_color = Some("hsl(220.5882352941, 60%, 88.3333333333%)".into());
    v.tertiary_color = Some("hsl(220.5882352941, 100%, 98.3333333333%)".into());
    v.tertiary_text_color = Some("rgb(8.5000000002, 5.7500000001, 0)".into());
    v.text_color = Some("#333".into());
    v.title_color = Some("rgb(8.5000000002, 5.7500000001, 0)".into());
    v.today_line_color = Some("red".into());
    v.transition_color = Some("#0b0b0b".into());
    v.transition_label_color = Some("#333".into());
    v.use_gradient = Some(true);
    v.venn1 = Some("hsl(40.5882352941, 100%, 63.3333333333%)".into());
    v.venn2 = Some("hsl(-79.4117647059, 100%, 63.3333333333%)".into());
    v.venn3 = Some("hsl(220.5882352941, 100%, 68.3333333333%)".into());
    v.venn4 = Some("hsl(100.5882352941, 100%, 63.3333333333%)".into());
    v.venn5 = Some("hsl(-19.4117647059, 100%, 63.3333333333%)".into());
    v.venn6 = Some("hsl(-19.4117647059, 100%, 63.3333333333%)".into());
    v.venn7 = Some("hsl(160.5882352941, 100%, 63.3333333333%)".into());
    v.venn8 = Some("hsl(40.5882352941, 100%, 63.3333333333%)".into());
    v.venn_set_text_color = Some("#333".into());
    v.venn_title_text_color = Some("rgb(8.5000000002, 5.7500000001, 0)".into());
    v.vert_line_color = Some("navy".into());
    v.radar = Some(RadarVars {
        axis_color: Some("#0b0b0b".into()),
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
        background_color: Some("#f4f4f4".into()),
        data_label_color: Some("#333".into()),
        plot_color_palette: Some("#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0".into()),
        title_color: Some("#333".into()),
        x_axis_label_color: Some("#333".into()),
        x_axis_line_color: Some("#333".into()),
        x_axis_tick_color: Some("#333".into()),
        x_axis_title_color: Some("#333".into()),
        y_axis_label_color: Some("#333".into()),
        y_axis_line_color: Some("#333".into()),
        y_axis_tick_color: Some("#333".into()),
        y_axis_title_color: Some("#333".into()),
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
        assert_eq!(v.primary_color.as_deref(), Some("#fff4dd"));
    }

    #[test]
    fn background_matches_upstream() {
        let v = variables();
        assert_eq!(v.background.as_deref(), Some("#f4f4f4"));
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
