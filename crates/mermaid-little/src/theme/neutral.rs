//! Mermaid's `neutral` theme — grayscale palette suited for
//! printing / mono targets. `contrast=#707070` drives most
//! greys.
//!
//! Seed values and derived colors mirror
//! `packages/mermaid/src/themes/theme-neutral.js` from upstream
//! mermaid@11.14.0. Derived fields were resolved ahead of time by
//! importing the upstream JS under a node shim and calling
//! `getThemeVariables()`; the resulting flat map is copied here as
//! literal constants. This keeps Wave 0 free of runtime color math.

use super::{RadarVars, ThemeVariables, XyChartVars};

#[allow(clippy::field_reassign_with_default, clippy::needless_update)]
/// Return a fully-populated [`ThemeVariables`] for the `neutral` theme.
#[must_use]
pub fn variables() -> ThemeVariables {
    let mut v = ThemeVariables::default();
    v.theme_color_limit = Some(12_i64);
    v.activation_bkg_color = Some("#f4f4f4".into());
    v.activation_border_color = Some("#666".into());
    v.active_task_bkg_color = Some("#eee".into());
    v.active_task_border_color = Some("hsl(0, 0%, 33.9215686275%)".into());
    v.actor_bkg = Some("#eee".into());
    v.actor_border = Some("hsl(0, 0%, 83%)".into());
    v.actor_line_color = Some("hsl(0, 0%, 83%)".into());
    v.actor_text_color = Some("#333".into());
    v.alt_background = Some("#f4f4f4".into());
    v.alt_section_bkg_color = Some("white".into());
    v.arch_edge_arrow_color = Some("#666".into());
    v.arch_edge_color = Some("#666".into());
    v.arch_edge_width = Some("3".into());
    v.arch_group_border_color = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.arch_group_border_width = Some("2px".into());
    v.arrowhead_color = Some("#333333".into());
    v.attribute_background_color_even = Some("#f2f2f2".into());
    v.attribute_background_color_odd = Some("#ffffff".into());
    v.background = Some("#ffffff".into());
    v.border1 = Some("#999".into());
    v.border2 = Some("#707070".into());
    v.branch_label_color = Some("#333".into());
    v.c_scale0 = Some("#555".into());
    v.c_scale1 = Some("#F4F4F4".into());
    v.c_scale10 = Some("#999".into());
    v.c_scale11 = Some("#777".into());
    v.c_scale2 = Some("#555".into());
    v.c_scale3 = Some("#BBB".into());
    v.c_scale4 = Some("#777".into());
    v.c_scale5 = Some("#999".into());
    v.c_scale6 = Some("#DDD".into());
    v.c_scale7 = Some("#FFF".into());
    v.c_scale8 = Some("#DDD".into());
    v.c_scale9 = Some("#BBB".into());
    v.c_scale_inv0 = Some("#aaaaaa".into());
    v.c_scale_inv1 = Some("#0b0b0b".into());
    v.c_scale_inv10 = Some("#666666".into());
    v.c_scale_inv11 = Some("#888888".into());
    v.c_scale_inv2 = Some("#aaaaaa".into());
    v.c_scale_inv3 = Some("#444444".into());
    v.c_scale_inv4 = Some("#888888".into());
    v.c_scale_inv5 = Some("#666666".into());
    v.c_scale_inv6 = Some("#222222".into());
    v.c_scale_inv7 = Some("#000000".into());
    v.c_scale_inv8 = Some("#222222".into());
    v.c_scale_inv9 = Some("#444444".into());
    v.c_scale_label0 = Some("#F4F4F4".into());
    v.c_scale_label1 = Some("#333".into());
    v.c_scale_label10 = Some("#333".into());
    v.c_scale_label11 = Some("#333".into());
    v.c_scale_label2 = Some("#F4F4F4".into());
    v.c_scale_label3 = Some("#333".into());
    v.c_scale_label4 = Some("#333".into());
    v.c_scale_label5 = Some("#333".into());
    v.c_scale_label6 = Some("#333".into());
    v.c_scale_label7 = Some("#333".into());
    v.c_scale_label8 = Some("#333".into());
    v.c_scale_label9 = Some("#333".into());
    v.c_scale_peer0 = Some("hsl(0, 0%, 23.3333333333%)".into());
    v.c_scale_peer1 = Some("hsl(0, 0%, 85.6862745098%)".into());
    v.c_scale_peer10 = Some("hsl(0, 0%, 50%)".into());
    v.c_scale_peer11 = Some("hsl(0, 0%, 36.6666666667%)".into());
    v.c_scale_peer2 = Some("hsl(0, 0%, 23.3333333333%)".into());
    v.c_scale_peer3 = Some("hsl(0, 0%, 63.3333333333%)".into());
    v.c_scale_peer4 = Some("hsl(0, 0%, 36.6666666667%)".into());
    v.c_scale_peer5 = Some("hsl(0, 0%, 50%)".into());
    v.c_scale_peer6 = Some("hsl(0, 0%, 76.6666666667%)".into());
    v.c_scale_peer7 = Some("hsl(0, 0%, 90%)".into());
    v.c_scale_peer8 = Some("hsl(0, 0%, 76.6666666667%)".into());
    v.c_scale_peer9 = Some("hsl(0, 0%, 63.3333333333%)".into());
    v.class_text = Some("#111111".into());
    v.cluster_bkg = Some("hsl(0, 0%, 98.9215686275%)".into());
    v.cluster_border = Some("#707070".into());
    v.commit_label_background = Some("hsl(0, 0%, 98.9215686275%)".into());
    v.commit_label_color = Some("rgb(2.7499999999, 2.7499999999, 2.7499999999)".into());
    v.commit_label_font_size = Some("10px".into());
    v.composite_background = Some("#ffffff".into());
    v.composite_title_background = Some("#eee".into());
    v.contrast = Some("#707070".into());
    v.crit_bkg_color = Some("#d42".into());
    v.crit_border_color = Some("hsl(10.9090909091, 73.3333333333%, 40%)".into());
    v.critical = Some("#d42".into());
    v.default_link_color = Some("#666".into());
    v.done = Some("#bbb".into());
    v.done_task_bkg_color = Some("#bbb".into());
    v.done_task_border_color = Some("#666".into());
    v.drop_shadow = Some("drop-shadow( 1px 2px 2px rgba(185,185,185,1))".into());
    v.edge_label_background = Some("white".into());
    v.error_bkg_color = Some("#552222".into());
    v.error_text_color = Some("#552222".into());
    v.exclude_bkg_color = Some("#eeeeee".into());
    v.fill_type0 = Some("#eee".into());
    v.fill_type1 = Some("hsl(0, 0%, 98.9215686275%)".into());
    v.fill_type2 = Some("hsl(64, 0%, 93.3333333333%)".into());
    v.fill_type3 = Some("hsl(64, 0%, 98.9215686275%)".into());
    v.fill_type4 = Some("hsl(-64, 0%, 93.3333333333%)".into());
    v.fill_type5 = Some("hsl(-64, 0%, 98.9215686275%)".into());
    v.fill_type6 = Some("hsl(128, 0%, 93.3333333333%)".into());
    v.fill_type7 = Some("hsl(128, 0%, 98.9215686275%)".into());
    v.font_family = Some("\"trebuchet ms\", verdana, arial, sans-serif".into());
    v.font_size = Some("16px".into());
    v.font_weight = Some("normal".into());
    v.git0 = Some("hsl(0, 0%, 70.6862745098%)".into());
    v.git1 = Some("#555".into());
    v.git2 = Some("#BBB".into());
    v.git3 = Some("#777".into());
    v.git4 = Some("#999".into());
    v.git5 = Some("#DDD".into());
    v.git6 = Some("#FFF".into());
    v.git7 = Some("#DDD".into());
    v.git_branch_label0 = Some("#333".into());
    v.git_branch_label1 = Some("white".into());
    v.git_branch_label2 = Some("#333".into());
    v.git_branch_label3 = Some("white".into());
    v.git_branch_label4 = Some("#333".into());
    v.git_branch_label5 = Some("#333".into());
    v.git_branch_label6 = Some("#333".into());
    v.git_branch_label7 = Some("#333".into());
    v.git_inv0 = Some("rgb(74.75, 74.75, 74.75)".into());
    v.git_inv1 = Some("#aaaaaa".into());
    v.git_inv2 = Some("#444444".into());
    v.git_inv3 = Some("#888888".into());
    v.git_inv4 = Some("#666666".into());
    v.git_inv5 = Some("#222222".into());
    v.git_inv6 = Some("#000000".into());
    v.git_inv7 = Some("#222222".into());
    v.gradient_start = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.gradient_stop = Some("hsl(0, 0%, 88.9215686275%)".into());
    v.grid_color = Some("hsl(0, 0%, 90%)".into());
    v.inner_end_background = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.label_background_color = Some("#eee".into());
    v.label_box_bkg_color = Some("#eee".into());
    v.label_box_border_color = Some("hsl(0, 0%, 83%)".into());
    v.label_color = Some("black".into());
    v.label_text_color = Some("#333".into());
    v.line_color = Some("#666".into());
    v.loop_text_color = Some("#333".into());
    v.main_bkg = Some("#eee".into());
    v.node_bkg = Some("#eee".into());
    v.node_border = Some("#999".into());
    v.note = Some("#ffa".into());
    v.note_bkg_color = Some("#666".into());
    v.note_border_color = Some("#999".into());
    v.note_font_weight = Some("normal".into());
    v.note_text_color = Some("#fff".into());
    v.person_bkg = Some("#eee".into());
    v.person_border = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.pie0 = Some("#555".into());
    v.pie1 = Some("#F4F4F4".into());
    v.pie10 = Some("#999".into());
    v.pie11 = Some("#777".into());
    v.pie12 = Some("#555".into());
    v.pie2 = Some("#555".into());
    v.pie3 = Some("#BBB".into());
    v.pie4 = Some("#777".into());
    v.pie5 = Some("#999".into());
    v.pie6 = Some("#DDD".into());
    v.pie7 = Some("#FFF".into());
    v.pie8 = Some("#DDD".into());
    v.pie9 = Some("#BBB".into());
    v.pie_legend_text_color = Some("#333".into());
    v.pie_legend_text_size = Some("17px".into());
    v.pie_opacity = Some("0.7".into());
    v.pie_outer_stroke_color = Some("black".into());
    v.pie_outer_stroke_width = Some("2px".into());
    v.pie_section_text_color = Some("#000000".into());
    v.pie_section_text_size = Some("17px".into());
    v.pie_stroke_color = Some("black".into());
    v.pie_stroke_width = Some("2px".into());
    v.pie_title_text_color = Some("#333".into());
    v.pie_title_text_size = Some("25px".into());
    v.primary_border_color = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.primary_color = Some("#eee".into());
    v.primary_text_color = Some("#111111".into());
    v.quadrant1_fill = Some("#eee".into());
    v.quadrant1_text_fill = Some("#111111".into());
    v.quadrant2_fill = Some("#f3f3f3".into());
    v.quadrant2_text_fill = Some("#0c0c0c".into());
    v.quadrant3_fill = Some("#f8f8f8".into());
    v.quadrant3_text_fill = Some("#070707".into());
    v.quadrant4_fill = Some("#fdfdfd".into());
    v.quadrant4_text_fill = Some("#020202".into());
    v.quadrant_external_border_stroke_fill = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.quadrant_internal_border_stroke_fill = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.quadrant_point_fill = Some("hsl(0, 0%, NaN%)".into());
    v.quadrant_point_text_fill = Some("#111111".into());
    v.quadrant_title_fill = Some("#111111".into());
    v.quadrant_x_axis_text_fill = Some("#111111".into());
    v.quadrant_y_axis_text_fill = Some("#111111".into());
    v.radius = Some(5_i64);
    v.relation_color = Some("#666".into());
    v.relation_label_background = Some("white".into());
    v.relation_label_color = Some("#333".into());
    v.requirement_background = Some("#eee".into());
    v.requirement_border_color = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.requirement_border_size = Some("1".into());
    v.requirement_text_color = Some("#111111".into());
    v.row_even = Some("#f4f4f4".into());
    v.row_odd = Some("hsl(0, 0%, 100%)".into());
    v.scale_label_color = Some("#333".into());
    v.second_bkg = Some("hsl(0, 0%, 98.9215686275%)".into());
    v.secondary_border_color = Some("hsl(0, 0%, 88.9215686275%)".into());
    v.secondary_color = Some("hsl(0, 0%, 98.9215686275%)".into());
    v.secondary_text_color = Some("rgb(2.7499999999, 2.7499999999, 2.7499999999)".into());
    v.section_bkg_color = Some("hsl(0, 0%, 73.9215686275%)".into());
    v.section_bkg_color2 = Some("hsl(0, 0%, 73.9215686275%)".into());
    v.sequence_number_color = Some("white".into());
    v.signal_color = Some("#333".into());
    v.signal_text_color = Some("#333".into());
    v.special_state_color = Some("#222".into());
    v.state_bkg = Some("#eee".into());
    v.state_border = Some("#000".into());
    v.state_label_color = Some("#111111".into());
    v.stroke_width = Some(1_i64);
    v.surface0 = Some("hsl(0, 0%, 88.3333333333%)".into());
    v.surface1 = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.surface2 = Some("hsl(0, 0%, 78.3333333333%)".into());
    v.surface3 = Some("hsl(0, 0%, 73.3333333333%)".into());
    v.surface4 = Some("hsl(0, 0%, 68.3333333333%)".into());
    v.surface_peer0 = Some("hsl(0, 0%, 85.3333333333%)".into());
    v.surface_peer1 = Some("hsl(0, 0%, 80.3333333333%)".into());
    v.surface_peer2 = Some("hsl(0, 0%, 75.3333333333%)".into());
    v.surface_peer3 = Some("hsl(0, 0%, 70.3333333333%)".into());
    v.surface_peer4 = Some("hsl(0, 0%, 65.3333333333%)".into());
    v.tag_label_background = Some("#eee".into());
    v.tag_label_border = Some("hsl(0, 0%, 83.3333333333%)".into());
    v.tag_label_color = Some("#111111".into());
    v.tag_label_font_size = Some("10px".into());
    v.task_bkg_color = Some("#707070".into());
    v.task_border_color = Some("hsl(0, 0%, 33.9215686275%)".into());
    v.task_text_clickable_color = Some("#003163".into());
    v.task_text_color = Some("white".into());
    v.task_text_dark_color = Some("#333".into());
    v.task_text_light_color = Some("white".into());
    v.task_text_outside_color = Some("#333".into());
    v.tertiary_border_color = Some("hsl(-160, 0%, 83.3333333333%)".into());
    v.tertiary_color = Some("hsl(-160, 0%, 93.3333333333%)".into());
    v.tertiary_text_color = Some("rgb(17.0000000001, 17.0000000001, 17.0000000001)".into());
    v.text = Some("#333".into());
    v.text_color = Some("#000000".into());
    v.title_color = Some("#333".into());
    v.today_line_color = Some("#d42".into());
    v.transition_color = Some("#000".into());
    v.transition_label_color = Some("#000000".into());
    v.use_gradient = Some(true);
    v.venn1 = Some("#555".into());
    v.venn2 = Some("#F4F4F4".into());
    v.venn3 = Some("#555".into());
    v.venn4 = Some("#BBB".into());
    v.venn5 = Some("#777".into());
    v.venn6 = Some("#999".into());
    v.venn7 = Some("#DDD".into());
    v.venn8 = Some("#FFF".into());
    v.venn_set_text_color = Some("#000000".into());
    v.venn_title_text_color = Some("#333".into());
    v.vert_line_color = Some("#d42".into());
    v.radar = Some(RadarVars {
        axis_color: Some("#666".into()),
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
        background_color: Some("#ffffff".into()),
        data_label_color: Some("#111111".into()),
        plot_color_palette: Some(
            "#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0".into(),
        ),
        title_color: Some("#111111".into()),
        x_axis_label_color: Some("#111111".into()),
        x_axis_line_color: Some("#111111".into()),
        x_axis_tick_color: Some("#111111".into()),
        x_axis_title_color: Some("#111111".into()),
        y_axis_label_color: Some("#111111".into()),
        y_axis_line_color: Some("#111111".into()),
        y_axis_tick_color: Some("#111111".into()),
        y_axis_title_color: Some("#111111".into()),
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
        assert_eq!(v.primary_color.as_deref(), Some("#eee"));
    }

    #[test]
    fn background_matches_upstream() {
        let v = variables();
        assert_eq!(v.background.as_deref(), Some("#ffffff"));
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
