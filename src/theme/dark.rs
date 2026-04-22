//! Mermaid's `dark` theme — intended for dark UI chrome.
//! Sequence-number text flips to black so it remains legible on the
//! light numbered bubbles.
//!
//! Seed values and derived colors mirror
//! `packages/mermaid/src/themes/theme-dark.js` from upstream
//! mermaid@11.14.0. Derived fields were resolved ahead of time by
//! importing the upstream JS under a node shim and calling
//! `getThemeVariables()`; the resulting flat map is copied here as
//! literal constants. This keeps Wave 0 free of runtime color math.

use super::{PacketVars, RadarVars, ThemeVariables, XyChartVars};

#[allow(clippy::field_reassign_with_default, clippy::needless_update)]
/// Return a fully-populated [`ThemeVariables`] for the `dark` theme.
#[must_use]
pub fn variables() -> ThemeVariables {
    let mut v = ThemeVariables::default();
    v.theme_color_limit = Some(12_i64);
    v.activation_bkg_color = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.activation_border_color = Some("#ccc".into());
    v.active_task_bkg_color = Some("#81B1DB".into());
    v.active_task_border_color = Some("#ffffff".into());
    v.actor_bkg = Some("#1f2020".into());
    v.actor_border = Some("#ccc".into());
    v.actor_line_color = Some("#ccc".into());
    v.actor_text_color = Some("lightgrey".into());
    v.alt_background = Some("#555".into());
    v.alt_section_bkg_color = Some("#333".into());
    v.arch_edge_arrow_color = Some("lightgrey".into());
    v.arch_edge_color = Some("lightgrey".into());
    v.arch_edge_width = Some("3".into());
    v.arch_group_border_color = Some("#cccccc".into());
    v.arch_group_border_width = Some("2px".into());
    v.arrowhead_color = Some("lightgrey".into());
    v.attribute_background_color_even = Some("hsl(0, 0%, 22%)".into());
    v.attribute_background_color_odd = Some("hsl(0, 0%, 32%)".into());
    v.background = Some("#333".into());
    v.border1 = Some("#ccc".into());
    v.border2 = Some("rgba(255, 255, 255, 0.25)".into());
    v.c_scale0 = Some("#1f2020".into());
    v.c_scale1 = Some("#0b0000".into());
    v.c_scale10 = Some("#00296f".into());
    v.c_scale11 = Some("#01629c".into());
    v.c_scale12 = Some("#010029".into());
    v.c_scale2 = Some("#4d1037".into());
    v.c_scale3 = Some("#3f5258".into());
    v.c_scale4 = Some("#4f2f1b".into());
    v.c_scale5 = Some("#6e0a0a".into());
    v.c_scale6 = Some("#3b0048".into());
    v.c_scale7 = Some("#995a01".into());
    v.c_scale8 = Some("#154706".into());
    v.c_scale9 = Some("#161722".into());
    v.c_scale_inv0 = Some("#e0dfdf".into());
    v.c_scale_inv1 = Some("#f4ffff".into());
    v.c_scale_inv10 = Some("#ffd690".into());
    v.c_scale_inv11 = Some("#fe9d63".into());
    v.c_scale_inv2 = Some("#b2efc8".into());
    v.c_scale_inv3 = Some("#c0ada7".into());
    v.c_scale_inv4 = Some("#b0d0e4".into());
    v.c_scale_inv5 = Some("#91f5f5".into());
    v.c_scale_inv6 = Some("#c4ffb7".into());
    v.c_scale_inv7 = Some("#66a5fe".into());
    v.c_scale_inv8 = Some("#eab8f9".into());
    v.c_scale_inv9 = Some("#e9e8dd".into());
    v.c_scale_label0 = Some("lightgrey".into());
    v.c_scale_label1 = Some("lightgrey".into());
    v.c_scale_label10 = Some("lightgrey".into());
    v.c_scale_label11 = Some("lightgrey".into());
    v.c_scale_label2 = Some("lightgrey".into());
    v.c_scale_label3 = Some("lightgrey".into());
    v.c_scale_label4 = Some("lightgrey".into());
    v.c_scale_label5 = Some("lightgrey".into());
    v.c_scale_label6 = Some("lightgrey".into());
    v.c_scale_label7 = Some("lightgrey".into());
    v.c_scale_label8 = Some("lightgrey".into());
    v.c_scale_label9 = Some("lightgrey".into());
    v.c_scale_peer0 = Some("hsl(180, 1.5873015873%, 22.3529411765%)".into());
    v.c_scale_peer1 = Some("hsl(0, 100%, 12.1568627451%)".into());
    v.c_scale_peer10 = Some("hsl(217.8378378378, 100%, 31.7647058824%)".into());
    v.c_scale_peer11 = Some("hsl(202.4516129032, 98.7261146497%, 40.7843137255%)".into());
    v.c_scale_peer2 = Some("hsl(321.6393442623, 65.5913978495%, 28.2352941176%)".into());
    v.c_scale_peer3 = Some("hsl(194.4, 16.5562913907%, 39.6078431373%)".into());
    v.c_scale_peer4 = Some("hsl(23.0769230769, 49.0566037736%, 30.7843137255%)".into());
    v.c_scale_peer5 = Some("hsl(0, 83.3333333333%, 33.5294117647%)".into());
    v.c_scale_peer6 = Some("hsl(289.1666666667, 100%, 24.1176470588%)".into());
    v.c_scale_peer7 = Some("hsl(35.1315789474, 98.7012987013%, 40.1960784314%)".into());
    v.c_scale_peer8 = Some("hsl(106.1538461538, 84.4155844156%, 25.0980392157%)".into());
    v.c_scale_peer9 = Some("hsl(235, 21.4285714286%, 20.9803921569%)".into());
    v.class_text = Some("#e0dfdf".into());
    v.cluster_bkg = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.cluster_border = Some("rgba(255, 255, 255, 0.25)".into());
    v.commit_label_background = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.commit_label_color = Some("rgb(183.8476190475, 181.5523809523, 181.5523809523)".into());
    v.commit_label_font_size = Some("10px".into());
    v.composite_background = Some("#333".into());
    v.composite_border = Some("#ccc".into());
    v.composite_title_background = Some("#1f2020".into());
    v.crit_bkg_color = Some("#E83737".into());
    v.crit_border_color = Some("#E83737".into());
    v.dark_text_color = Some("hsl(28.5714285714, 17.3553719008%, 86.2745098039%)".into());
    v.default_link_color = Some("lightgrey".into());
    v.done_task_bkg_color = Some("lightgrey".into());
    v.done_task_border_color = Some("grey".into());
    v.drop_shadow = Some("drop-shadow( 1px 2px 2px rgba(185,185,185,1))".into());
    v.edge_label_background = Some("hsl(0, 0%, 34.4117647059%)".into());
    v.error_bkg_color = Some("#a44141".into());
    v.error_text_color = Some("#ddd".into());
    v.exclude_bkg_color = Some("hsl(52.9411764706, 28.813559322%, 48.431372549%)".into());
    v.fill_type0 = Some("#1f2020".into());
    v.fill_type1 = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.fill_type2 = Some("hsl(244, 1.5873015873%, 12.3529411765%)".into());
    v.fill_type3 = Some("hsl(244, 1.5873015873%, 28.3529411765%)".into());
    v.fill_type4 = Some("hsl(116, 1.5873015873%, 12.3529411765%)".into());
    v.fill_type5 = Some("hsl(116, 1.5873015873%, 28.3529411765%)".into());
    v.fill_type6 = Some("hsl(308, 1.5873015873%, 12.3529411765%)".into());
    v.fill_type7 = Some("hsl(308, 1.5873015873%, 28.3529411765%)".into());
    v.font_family = Some("\"trebuchet ms\", verdana, arial, sans-serif".into());
    v.font_size = Some("16px".into());
    v.font_weight = Some("normal".into());
    v.git0 = Some("hsl(180, 1.5873015873%, 48.3529411765%)".into());
    v.git1 = Some("hsl(321.6393442623, 65.5913978495%, 38.2352941176%)".into());
    v.git2 = Some("hsl(194.4, 16.5562913907%, 49.6078431373%)".into());
    v.git3 = Some("hsl(23.0769230769, 49.0566037736%, 40.7843137255%)".into());
    v.git4 = Some("hsl(0, 83.3333333333%, 43.5294117647%)".into());
    v.git5 = Some("hsl(289.1666666667, 100%, 24.1176470588%)".into());
    v.git6 = Some("hsl(35.1315789474, 98.7012987013%, 40.1960784314%)".into());
    v.git7 = Some("hsl(106.1538461538, 84.4155844156%, 35.0980392157%)".into());
    v.git_branch_label0 = Some("#2c2c2c".into());
    v.git_branch_label1 = Some("lightgrey".into());
    v.git_branch_label2 = Some("lightgrey".into());
    v.git_branch_label3 = Some("#2c2c2c".into());
    v.git_branch_label4 = Some("lightgrey".into());
    v.git_branch_label5 = Some("lightgrey".into());
    v.git_branch_label6 = Some("lightgrey".into());
    v.git_branch_label7 = Some("lightgrey".into());
    v.git_inv0 = Some("rgb(133.6571428571, 129.7428571428, 129.7428571428)".into());
    v.git_inv1 = Some("rgb(93.5483870969, 221.4516129033, 139.677419355)".into());
    v.git_inv2 = Some("rgb(149.4437086091, 117.6092715231, 107.5562913906)".into());
    v.git_inv3 = Some("rgb(99.9811320754, 162.7735849057, 202.0188679245)".into());
    v.git_inv4 = Some("rgb(51.5000000001, 236.5, 236.5)".into());
    v.git_inv5 = Some("rgb(154.2083333334, 255, 132.0000000001)".into());
    v.git_inv6 = Some("rgb(51.331168831, 135.1948051946, 253.6688311688)".into());
    v.git_inv7 = Some("rgb(206.1818181817, 89.948051948, 241.051948052)".into());
    v.gradient_start = Some("#cccccc".into());
    v.gradient_stop = Some("hsl(180, 0%, 18.3529411765%)".into());
    v.grid_color = Some("lightgrey".into());
    v.inner_end_background = Some("#cccccc".into());
    v.label_background = Some("#181818".into());
    v.label_background_color = Some("#1f2020".into());
    v.label_box_bkg_color = Some("#1f2020".into());
    v.label_box_border_color = Some("#ccc".into());
    v.label_color = Some("calculated".into());
    v.label_text_color = Some("lightgrey".into());
    v.line_color = Some("lightgrey".into());
    v.loop_text_color = Some("lightgrey".into());
    v.main_bkg = Some("#1f2020".into());
    v.main_contrast_color = Some("lightgrey".into());
    v.node_bkg = Some("#1f2020".into());
    v.node_border = Some("#ccc".into());
    v.note_bkg_color = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.note_border_color = Some("hsl(180, 0%, 18.3529411765%)".into());
    v.note_font_weight = Some("normal".into());
    v.note_text_color = Some("rgb(183.8476190475, 181.5523809523, 181.5523809523)".into());
    v.person_bkg = Some("#1f2020".into());
    v.person_border = Some("#cccccc".into());
    v.pie0 = Some("#1f2020".into());
    v.pie1 = Some("#0b0000".into());
    v.pie10 = Some("#00296f".into());
    v.pie11 = Some("#01629c".into());
    v.pie2 = Some("#4d1037".into());
    v.pie3 = Some("#3f5258".into());
    v.pie4 = Some("#4f2f1b".into());
    v.pie5 = Some("#6e0a0a".into());
    v.pie6 = Some("#3b0048".into());
    v.pie7 = Some("#995a01".into());
    v.pie8 = Some("#154706".into());
    v.pie9 = Some("#161722".into());
    v.pie_legend_text_color = Some("lightgrey".into());
    v.pie_legend_text_size = Some("17px".into());
    v.pie_opacity = Some("0.7".into());
    v.pie_outer_stroke_color = Some("black".into());
    v.pie_outer_stroke_width = Some("2px".into());
    v.pie_section_text_color = Some("#ccc".into());
    v.pie_section_text_size = Some("17px".into());
    v.pie_stroke_color = Some("black".into());
    v.pie_stroke_width = Some("2px".into());
    v.pie_title_text_color = Some("lightgrey".into());
    v.pie_title_text_size = Some("25px".into());
    v.primary_border_color = Some("#cccccc".into());
    v.primary_color = Some("#1f2020".into());
    v.primary_text_color = Some("#e0dfdf".into());
    v.quadrant1_fill = Some("#1f2020".into());
    v.quadrant1_text_fill = Some("#e0dfdf".into());
    v.quadrant2_fill = Some("#242525".into());
    v.quadrant2_text_fill = Some("#dbdada".into());
    v.quadrant3_fill = Some("#292a2a".into());
    v.quadrant3_text_fill = Some("#d6d5d5".into());
    v.quadrant4_fill = Some("#2e2f2f".into());
    v.quadrant4_text_fill = Some("#d1d0d0".into());
    v.quadrant_external_border_stroke_fill = Some("#cccccc".into());
    v.quadrant_internal_border_stroke_fill = Some("#cccccc".into());
    v.quadrant_point_fill = Some("hsl(180, 1.5873015873%, NaN%)".into());
    v.quadrant_point_text_fill = Some("#e0dfdf".into());
    v.quadrant_title_fill = Some("#e0dfdf".into());
    v.quadrant_x_axis_text_fill = Some("#e0dfdf".into());
    v.quadrant_y_axis_text_fill = Some("#e0dfdf".into());
    v.radius = Some(5_i64);
    v.relation_color = Some("lightgrey".into());
    v.relation_label_background = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.relation_label_color = Some("lightgrey".into());
    v.requirement_background = Some("#1f2020".into());
    v.requirement_border_color = Some("#cccccc".into());
    v.requirement_border_size = Some("1".into());
    v.requirement_text_color = Some("#e0dfdf".into());
    v.row_even = Some("hsl(180, 1.5873015873%, 2.3529411765%)".into());
    v.row_odd = Some("hsl(180, 1.5873015873%, 17.3529411765%)".into());
    v.scale_label_color = Some("lightgrey".into());
    v.second_bkg = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.secondary_border_color = Some("hsl(180, 0%, 18.3529411765%)".into());
    v.secondary_color = Some("hsl(180, 1.5873015873%, 28.3529411765%)".into());
    v.secondary_text_color = Some("rgb(183.8476190475, 181.5523809523, 181.5523809523)".into());
    v.section_bkg_color = Some("hsl(52.9411764706, 28.813559322%, 58.431372549%)".into());
    v.section_bkg_color2 = Some("#EAE8D9".into());
    v.sequence_number_color = Some("black".into());
    v.signal_color = Some("lightgrey".into());
    v.signal_text_color = Some("lightgrey".into());
    v.special_state_color = Some("#f4f4f4".into());
    v.state_bkg = Some("#1f2020".into());
    v.state_label_color = Some("#e0dfdf".into());
    v.stroke_width = Some(1_i64);
    v.surface0 = Some("hsl(210, 0%, 22.3529411765%)".into());
    v.surface1 = Some("hsl(210, 0%, 18.3529411765%)".into());
    v.surface2 = Some("hsl(210, 0%, 14.3529411765%)".into());
    v.surface3 = Some("hsl(210, 0%, 10.3529411765%)".into());
    v.surface4 = Some("hsl(210, 0%, 6.3529411765%)".into());
    v.surface_peer0 = Some("hsl(210, 0%, 19.3529411765%)".into());
    v.surface_peer1 = Some("hsl(210, 0%, 15.3529411765%)".into());
    v.surface_peer2 = Some("hsl(210, 0%, 11.3529411765%)".into());
    v.surface_peer3 = Some("hsl(210, 0%, 7.3529411765%)".into());
    v.surface_peer4 = Some("hsl(210, 0%, 3.3529411765%)".into());
    v.tag_label_background = Some("#1f2020".into());
    v.tag_label_border = Some("#cccccc".into());
    v.tag_label_color = Some("#e0dfdf".into());
    v.tag_label_font_size = Some("10px".into());
    v.task_bkg_color = Some("hsl(180, 1.5873015873%, 35.3529411765%)".into());
    v.task_border_color = Some("#ffffff".into());
    v.task_text_clickable_color = Some("#003163".into());
    v.task_text_color = Some("hsl(28.5714285714, 17.3553719008%, 86.2745098039%)".into());
    v.task_text_dark_color = Some("#2c2c2c".into());
    v.task_text_light_color = Some("lightgrey".into());
    v.task_text_outside_color = Some("lightgrey".into());
    v.tertiary_border_color = Some("hsl(20, 0%, 2.3529411765%)".into());
    v.tertiary_color = Some("hsl(20, 1.5873015873%, 12.3529411765%)".into());
    v.tertiary_text_color = Some("rgb(222.9999999999, 223.6666666666, 223.9999999999)".into());
    v.text_color = Some("#ccc".into());
    v.title_color = Some("#F9FFFE".into());
    v.today_line_color = Some("#DB5757".into());
    v.transition_color = Some("lightgrey".into());
    v.transition_label_color = Some("#ccc".into());
    v.use_gradient = Some(true);
    v.venn1 = Some("hsl(180, 1.5873015873%, 42.3529411765%)".into());
    v.venn2 = Some("hsl(0, 100%, 32.1568627451%)".into());
    v.venn3 = Some("hsl(321.6393442623, 65.5913978495%, 48.2352941176%)".into());
    v.venn4 = Some("hsl(194.4, 16.5562913907%, 59.6078431373%)".into());
    v.venn5 = Some("hsl(23.0769230769, 49.0566037736%, 50.7843137255%)".into());
    v.venn6 = Some("hsl(0, 83.3333333333%, 53.5294117647%)".into());
    v.venn7 = Some("hsl(289.1666666667, 100%, 44.1176470588%)".into());
    v.venn8 = Some("hsl(35.1315789474, 98.7012987013%, 60.1960784314%)".into());
    v.venn_set_text_color = Some("#ccc".into());
    v.venn_title_text_color = Some("#F9FFFE".into());
    v.vert_line_color = Some("#00BFFF".into());
    v.packet = Some(PacketVars {
        block_fill_color: Some("#333".into()),
        block_stroke_color: Some("#e0dfdf".into()),
        end_byte_color: Some("#e0dfdf".into()),
        label_color: Some("#e0dfdf".into()),
        start_byte_color: Some("#e0dfdf".into()),
        title_color: Some("#e0dfdf".into()),
        ..Default::default()
    });
    v.radar = Some(RadarVars {
        axis_color: Some("lightgrey".into()),
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
        background_color: Some("#333".into()),
        data_label_color: Some("#e0dfdf".into()),
        plot_color_palette: Some("#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22".into()),
        title_color: Some("#e0dfdf".into()),
        x_axis_label_color: Some("#e0dfdf".into()),
        x_axis_line_color: Some("#e0dfdf".into()),
        x_axis_tick_color: Some("#e0dfdf".into()),
        x_axis_title_color: Some("#e0dfdf".into()),
        y_axis_label_color: Some("#e0dfdf".into()),
        y_axis_line_color: Some("#e0dfdf".into()),
        y_axis_tick_color: Some("#e0dfdf".into()),
        y_axis_title_color: Some("#e0dfdf".into()),
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
        assert_eq!(v.primary_color.as_deref(), Some("#1f2020"));
    }

    #[test]
    fn background_matches_upstream() {
        let v = variables();
        assert_eq!(v.background.as_deref(), Some("#333"));
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
