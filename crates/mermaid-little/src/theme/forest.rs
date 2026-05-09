//! Mermaid's `forest` theme — green-dominant palette with hard
//! black line colour. Uses `border1=#13540c`, `border2=#6eaa49`.
//!
//! Seed values and derived colors mirror
//! `packages/mermaid/src/themes/theme-forest.js` from upstream
//! mermaid@11.14.0. Derived fields were resolved ahead of time by
//! importing the upstream JS under a node shim and calling
//! `getThemeVariables()`; the resulting flat map is copied here as
//! literal constants. This keeps Wave 0 free of runtime color math.

use super::{PacketVars, RadarVars, ThemeVariables, XyChartVars};

#[allow(clippy::field_reassign_with_default, clippy::needless_update)]
/// Return a fully-populated [`ThemeVariables`] for the `forest` theme.
#[must_use]
pub fn variables() -> ThemeVariables {
    let mut v = ThemeVariables::default();
    v.theme_color_limit = Some(12_i64);
    v.activation_bkg_color = Some("#f4f4f4".into());
    v.activation_border_color = Some("#666".into());
    v.active_task_bkg_color = Some("#cde498".into());
    v.active_task_border_color = Some("#13540c".into());
    v.actor_bkg = Some("#cde498".into());
    v.actor_border = Some("hsl(78.1578947368, 58.4615384615%, 54.5098039216%)".into());
    v.actor_line_color = Some("hsl(78.1578947368, 58.4615384615%, 54.5098039216%)".into());
    v.actor_text_color = Some("black".into());
    v.alt_background = Some("#f0f0f0".into());
    v.alt_section_bkg_color = Some("white".into());
    v.arch_edge_arrow_color = Some("#000000".into());
    v.arch_edge_color = Some("#000000".into());
    v.arch_edge_width = Some("3".into());
    v.arch_group_border_color = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.arch_group_border_width = Some("2px".into());
    v.arrowhead_color = Some("green".into());
    v.attribute_background_color_even = Some("#f2f2f2".into());
    v.attribute_background_color_odd = Some("#ffffff".into());
    v.background = Some("white".into());
    v.border1 = Some("#13540c".into());
    v.border2 = Some("#6eaa49".into());
    v.c_scale0 = Some("hsl(78.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale1 = Some("hsl(98.961038961, 100%, 74.9019607843%)".into());
    v.c_scale10 = Some("hsl(18.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale11 = Some("hsl(48.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale2 = Some("hsl(78.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.c_scale3 = Some("hsl(108.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale4 = Some("hsl(138.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale5 = Some("hsl(168.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale6 = Some("hsl(198.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale7 = Some("hsl(228.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale8 = Some("hsl(288.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale9 = Some("hsl(348.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv0 = Some("hsl(258.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv1 = Some("hsl(278.961038961, 100%, 74.9019607843%)".into());
    v.c_scale_inv10 = Some("hsl(198.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv11 = Some("hsl(228.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv2 = Some("hsl(258.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.c_scale_inv3 = Some("hsl(288.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv4 = Some("hsl(318.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv5 = Some("hsl(348.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv6 = Some("hsl(18.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv7 = Some("hsl(48.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv8 = Some("hsl(108.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_inv9 = Some("hsl(168.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.c_scale_label0 = Some("black".into());
    v.c_scale_label1 = Some("black".into());
    v.c_scale_label10 = Some("black".into());
    v.c_scale_label11 = Some("black".into());
    v.c_scale_label2 = Some("black".into());
    v.c_scale_label3 = Some("black".into());
    v.c_scale_label4 = Some("black".into());
    v.c_scale_label5 = Some("black".into());
    v.c_scale_label6 = Some("black".into());
    v.c_scale_label7 = Some("black".into());
    v.c_scale_label8 = Some("black".into());
    v.c_scale_label9 = Some("black".into());
    v.c_scale_peer0 = Some("hsl(78.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer1 = Some("hsl(98.961038961, 100%, 39.9019607843%)".into());
    v.c_scale_peer10 = Some("hsl(18.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer11 = Some("hsl(48.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer2 = Some("hsl(78.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.c_scale_peer3 = Some("hsl(108.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer4 = Some("hsl(138.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer5 = Some("hsl(168.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer6 = Some("hsl(198.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer7 = Some("hsl(228.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer8 = Some("hsl(288.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.c_scale_peer9 = Some("hsl(348.1578947368, 58.4615384615%, 39.5098039216%)".into());
    v.class_text = Some("#321b67".into());
    v.cluster_bkg = Some("#cdffb2".into());
    v.cluster_border = Some("#6eaa49".into());
    v.commit_label_background = Some("#cdffb2".into());
    v.commit_label_color = Some("#32004d".into());
    v.commit_label_font_size = Some("10px".into());
    v.composite_background = Some("white".into());
    v.composite_border = Some("#13540c".into());
    v.composite_title_background = Some("#cde498".into());
    v.crit_bkg_color = Some("red".into());
    v.crit_border_color = Some("#ff8888".into());
    v.default_link_color = Some("#000000".into());
    v.done_task_bkg_color = Some("lightgrey".into());
    v.done_task_border_color = Some("grey".into());
    v.drop_shadow = Some("drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))".into());
    v.edge_label_background = Some("#e8e8e8".into());
    v.error_bkg_color = Some("#552222".into());
    v.error_text_color = Some("#552222".into());
    v.exclude_bkg_color = Some("#eeeeee".into());
    v.fill_type0 = Some("#cde498".into());
    v.fill_type1 = Some("#cdffb2".into());
    v.fill_type2 = Some("hsl(142.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.fill_type3 = Some("hsl(162.961038961, 100%, 84.9019607843%)".into());
    v.fill_type4 = Some("hsl(14.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.fill_type5 = Some("hsl(34.961038961, 100%, 84.9019607843%)".into());
    v.fill_type6 = Some("hsl(206.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.fill_type7 = Some("hsl(226.961038961, 100%, 84.9019607843%)".into());
    v.font_family = Some("\"trebuchet ms\", verdana, arial, sans-serif".into());
    v.font_size = Some("16px".into());
    v.font_weight = Some("normal".into());
    v.git0 = Some("hsl(78.1578947368, 58.4615384615%, 49.5098039216%)".into());
    v.git1 = Some("hsl(98.961038961, 100%, 59.9019607843%)".into());
    v.git2 = Some("hsl(78.1578947368, 58.4615384615%, 59.5098039216%)".into());
    v.git3 = Some("hsl(48.1578947368, 58.4615384615%, 49.5098039216%)".into());
    v.git4 = Some("hsl(18.1578947368, 58.4615384615%, 49.5098039216%)".into());
    v.git5 = Some("hsl(-11.8421052632, 58.4615384615%, 49.5098039216%)".into());
    v.git6 = Some("hsl(138.1578947368, 58.4615384615%, 49.5098039216%)".into());
    v.git7 = Some("hsl(198.1578947368, 58.4615384615%, 49.5098039216%)".into());
    v.git_branch_label0 = Some("#ffffff".into());
    v.git_branch_label1 = Some("black".into());
    v.git_branch_label2 = Some("black".into());
    v.git_branch_label3 = Some("#ffffff".into());
    v.git_branch_label4 = Some("black".into());
    v.git_branch_label5 = Some("black".into());
    v.git_branch_label6 = Some("black".into());
    v.git_branch_label7 = Some("black".into());
    v.git_inv0 = Some("rgb(99.6153846152, 54.9423076922, 202.5576923076)".into());
    v.git_inv1 = Some("rgb(132.7922077921, 0, 204.5000000001)".into());
    v.git_inv2 = Some("rgb(79.4230769229, 42.8884615385, 163.6115384614)".into());
    v.git_inv3 = Some("rgb(54.9423076922, 84.0769230769, 202.5576923076)".into());
    v.git_inv4 = Some("rgb(54.9423076922, 157.8846153846, 202.5576923076)".into());
    v.git_inv5 = Some("rgb(54.9423076922, 202.5576923076, 173.4230769229)".into());
    v.git_inv6 = Some("rgb(202.5576923076, 54.9423076922, 157.8846153846)".into());
    v.git_inv7 = Some("rgb(202.5576923076, 99.6153846152, 54.9423076922)".into());
    v.gradient_start = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.gradient_stop = Some("hsl(98.961038961, 60%, 74.9019607843%)".into());
    v.grid_color = Some("lightgrey".into());
    v.inner_end_background = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.label_background_color = Some("#cde498".into());
    v.label_box_bkg_color = Some("#cde498".into());
    v.label_box_border_color = Some("#326932".into());
    v.label_color = Some("black".into());
    v.label_text_color = Some("black".into());
    v.line_color = Some("#000000".into());
    v.loop_text_color = Some("black".into());
    v.main_bkg = Some("#cde498".into());
    v.node_bkg = Some("#cde498".into());
    v.node_border = Some("#13540c".into());
    v.note_bkg_color = Some("#fff5ad".into());
    v.note_border_color = Some("#6eaa49".into());
    v.note_font_weight = Some("normal".into());
    v.note_text_color = Some("black".into());
    v.person_bkg = Some("#cde498".into());
    v.person_border = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.pie1 = Some("#cde498".into());
    v.pie10 = Some("hsl(138.1578947368, 58.4615384615%, 24.5098039216%)".into());
    v.pie11 = Some("hsl(18.1578947368, 58.4615384615%, 24.5098039216%)".into());
    v.pie12 = Some("hsl(198.1578947368, 58.4615384615%, 24.5098039216%)".into());
    v.pie2 = Some("#cdffb2".into());
    v.pie3 = Some("hsl(78.1578947368, 58.4615384615%, 84.5098039216%)".into());
    v.pie4 = Some("hsl(78.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.pie5 = Some("hsl(98.961038961, 100%, 54.9019607843%)".into());
    v.pie6 = Some("hsl(118.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.pie7 = Some("hsl(138.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.pie8 = Some("hsl(18.1578947368, 58.4615384615%, 64.5098039216%)".into());
    v.pie9 = Some("hsl(198.1578947368, 58.4615384615%, 74.5098039216%)".into());
    v.pie_legend_text_color = Some("black".into());
    v.pie_legend_text_size = Some("17px".into());
    v.pie_opacity = Some("0.7".into());
    v.pie_outer_stroke_color = Some("black".into());
    v.pie_outer_stroke_width = Some("2px".into());
    v.pie_section_text_color = Some("#000000".into());
    v.pie_section_text_size = Some("17px".into());
    v.pie_stroke_color = Some("black".into());
    v.pie_stroke_width = Some("2px".into());
    v.pie_title_text_color = Some("black".into());
    v.pie_title_text_size = Some("25px".into());
    v.primary_border_color = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.primary_color = Some("#cde498".into());
    v.primary_text_color = Some("#321b67".into());
    v.quadrant1_fill = Some("#cde498".into());
    v.quadrant1_text_fill = Some("#321b67".into());
    v.quadrant2_fill = Some("#d2e99d".into());
    v.quadrant2_text_fill = Some("#2d1662".into());
    v.quadrant3_fill = Some("#d7eea2".into());
    v.quadrant3_text_fill = Some("#28115d".into());
    v.quadrant4_fill = Some("#dcf3a7".into());
    v.quadrant4_text_fill = Some("#230c58".into());
    v.quadrant_external_border_stroke_fill =
        Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.quadrant_internal_border_stroke_fill =
        Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.quadrant_point_fill = Some("hsl(78.1578947368, 58.4615384615%, NaN%)".into());
    v.quadrant_point_text_fill = Some("#321b67".into());
    v.quadrant_title_fill = Some("#321b67".into());
    v.quadrant_x_axis_text_fill = Some("#321b67".into());
    v.quadrant_y_axis_text_fill = Some("#321b67".into());
    v.radius = Some(5_i64);
    v.relation_color = Some("#000000".into());
    v.relation_label_background = Some("#e8e8e8".into());
    v.relation_label_color = Some("black".into());
    v.requirement_background = Some("#cde498".into());
    v.requirement_border_color = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.requirement_border_size = Some("1".into());
    v.requirement_text_color = Some("#321b67".into());
    v.row_even = Some("hsl(78.1578947368, 58.4615384615%, 94.5098039216%)".into());
    v.row_odd = Some("hsl(78.1578947368, 58.4615384615%, 100%)".into());
    v.scale_label_color = Some("black".into());
    v.second_bkg = Some("#cdffb2".into());
    v.secondary_border_color = Some("hsl(98.961038961, 60%, 74.9019607843%)".into());
    v.secondary_color = Some("#cdffb2".into());
    v.secondary_text_color = Some("#32004d".into());
    v.section_bkg_color = Some("#6eaa49".into());
    v.section_bkg_color2 = Some("#6eaa49".into());
    v.sequence_number_color = Some("white".into());
    v.signal_color = Some("#333".into());
    v.signal_text_color = Some("#333".into());
    v.special_state_color = Some("#000000".into());
    v.state_bkg = Some("#cde498".into());
    v.state_label_color = Some("#321b67".into());
    v.stroke_width = Some(1_i64);
    v.surface0 = Some("hsl(108.1578947368, 28.4615384615%, 69.5098039216%)".into());
    v.surface1 = Some("hsl(108.1578947368, 28.4615384615%, 64.5098039216%)".into());
    v.surface2 = Some("hsl(108.1578947368, 28.4615384615%, 59.5098039216%)".into());
    v.surface3 = Some("hsl(108.1578947368, 28.4615384615%, 54.5098039216%)".into());
    v.surface4 = Some("hsl(108.1578947368, 28.4615384615%, 49.5098039216%)".into());
    v.surface_peer0 = Some("hsl(108.1578947368, 28.4615384615%, 66.5098039216%)".into());
    v.surface_peer1 = Some("hsl(108.1578947368, 28.4615384615%, 61.5098039216%)".into());
    v.surface_peer2 = Some("hsl(108.1578947368, 28.4615384615%, 56.5098039216%)".into());
    v.surface_peer3 = Some("hsl(108.1578947368, 28.4615384615%, 51.5098039216%)".into());
    v.surface_peer4 = Some("hsl(108.1578947368, 28.4615384615%, 46.5098039216%)".into());
    v.tag_label_background = Some("#cde498".into());
    v.tag_label_border = Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)".into());
    v.tag_label_color = Some("#321b67".into());
    v.tag_label_font_size = Some("10px".into());
    v.task_bkg_color = Some("#487e3a".into());
    v.task_border_color = Some("#13540c".into());
    v.task_text_clickable_color = Some("#003163".into());
    v.task_text_color = Some("white".into());
    v.task_text_dark_color = Some("black".into());
    v.task_text_light_color = Some("white".into());
    v.task_text_outside_color = Some("black".into());
    v.tertiary_border_color = Some("hsl(78.1578947368, 18.4615384615%, 74.5098039216%)".into());
    v.tertiary_color = Some("hsl(78.1578947368, 58.4615384615%, 84.5098039216%)".into());
    v.tertiary_text_color = Some("#321b67".into());
    v.text_color = Some("#000000".into());
    v.title_color = Some("#333".into());
    v.today_line_color = Some("red".into());
    v.transition_color = Some("#000000".into());
    v.transition_label_color = Some("#000000".into());
    v.use_gradient = Some(true);
    v.venn1 = Some("hsl(78.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.venn2 = Some("hsl(98.961038961, 100%, 54.9019607843%)".into());
    v.venn3 = Some("hsl(78.1578947368, 58.4615384615%, 54.5098039216%)".into());
    v.venn4 = Some("hsl(138.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.venn5 = Some("hsl(18.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.venn6 = Some("hsl(158.961038961, 100%, 54.9019607843%)".into());
    v.venn7 = Some("hsl(198.1578947368, 58.4615384615%, 44.5098039216%)".into());
    v.venn8 = Some("hsl(218.961038961, 100%, 54.9019607843%)".into());
    v.venn_set_text_color = Some("#000000".into());
    v.venn_title_text_color = Some("#333".into());
    v.vert_line_color = Some("#00BFFF".into());
    v.packet = Some(PacketVars {
        block_fill_color: Some("#cde498".into()),
        block_stroke_color: Some("#321b67".into()),
        end_byte_color: Some("#321b67".into()),
        label_color: Some("#321b67".into()),
        start_byte_color: Some("#321b67".into()),
        title_color: Some("#321b67".into()),
        ..Default::default()
    });
    v.radar = Some(RadarVars {
        axis_color: Some("#000000".into()),
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
        data_label_color: Some("#321b67".into()),
        plot_color_palette: Some(
            "#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"
                .into(),
        ),
        title_color: Some("#321b67".into()),
        x_axis_label_color: Some("#321b67".into()),
        x_axis_line_color: Some("#321b67".into()),
        x_axis_tick_color: Some("#321b67".into()),
        x_axis_title_color: Some("#321b67".into()),
        y_axis_label_color: Some("#321b67".into()),
        y_axis_line_color: Some("#321b67".into()),
        y_axis_tick_color: Some("#321b67".into()),
        y_axis_title_color: Some("#321b67".into()),
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
        assert_eq!(v.primary_color.as_deref(), Some("#cde498"));
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
