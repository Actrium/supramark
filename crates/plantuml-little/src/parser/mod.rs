pub mod activity;
pub mod board;
pub mod bpm;
pub mod chart;
pub mod chronology;
pub mod class;
pub mod common;
pub mod component;
pub mod creole;
pub mod creole_diagram;
pub mod ditaa;
pub mod dot;
pub mod ebnf;
pub mod erd;
pub mod files_diagram;
pub mod flow;
pub mod gantt;
pub mod git;
pub mod hcl;
pub mod json_diagram;
pub mod math;
pub mod mindmap;
pub mod nwdiag;
pub mod packet;
pub mod pie;
pub mod regex_diagram;
pub mod salt;
pub mod sequence;
pub mod state;
pub mod timing;
pub mod usecase;
pub mod wbs;
pub mod wire;
pub mod yaml;

use crate::model::diagram::ClassDiagram;
use crate::model::Diagram;
use crate::Result;

pub fn parse(source: &str) -> Result<Diagram> {
    parse_with_original(source, None)
}

pub fn parse_with_original(source: &str, original_source: Option<&str>) -> Result<Diagram> {
    // First check for specialized @start tags
    let tag_hint = common::detect_start_tag(source);
    if let Some(hint) = tag_hint {
        return match hint {
            DiagramHint::Bpm => {
                let bd = bpm::parse_bpm_diagram(source)?;
                Ok(Diagram::Bpm(bd))
            }
            DiagramHint::Def => {
                // Java PSystemDefinition renders just the @startdef line as text
                let start_line = source
                    .lines()
                    .find(|l| l.trim().starts_with("@startdef"))
                    .unwrap_or("@startdef")
                    .trim()
                    .to_string();
                Ok(Diagram::Def(crate::model::math::MathDiagram {
                    formula: start_line,
                }))
            }
            DiagramHint::Chart => {
                let cd = chart::parse_chart_diagram(source)?;
                Ok(Diagram::Chart(cd))
            }
            DiagramHint::Erd => {
                let ed = erd::parse_erd_diagram(source)?;
                Ok(Diagram::Erd(ed))
            }
            DiagramHint::Files => {
                let fd = files_diagram::parse_files_diagram(source)?;
                Ok(Diagram::Files(fd))
            }
            DiagramHint::Flow => {
                let fd = flow::parse_flow_diagram(source)?;
                Ok(Diagram::Flow(fd))
            }
            DiagramHint::Gantt => {
                let gd = gantt::parse_gantt_diagram(source)?;
                Ok(Diagram::Gantt(gd))
            }
            DiagramHint::Jcckit => Err(crate::Error::BinaryPngDiagram("JCCKIT".into())),
            DiagramHint::Ditaa => {
                let dd = ditaa::parse_ditaa(source)?;
                Ok(Diagram::Ditaa(dd))
            }
            DiagramHint::Json => {
                let jd = json_diagram::parse_json_diagram(source)?;
                Ok(Diagram::Json(jd))
            }
            DiagramHint::Mindmap => {
                let md = mindmap::parse_mindmap_diagram(source)?;
                Ok(Diagram::Mindmap(md))
            }
            DiagramHint::Nwdiag => {
                let nd = nwdiag::parse_nwdiag_diagram(source)?;
                Ok(Diagram::Nwdiag(nd))
            }
            DiagramHint::Salt => {
                let sd = salt::parse_salt_diagram(source)?;
                Ok(Diagram::Salt(sd))
            }
            DiagramHint::Wbs => {
                let wd = wbs::parse_wbs_diagram(source)?;
                Ok(Diagram::Wbs(wd))
            }
            DiagramHint::Yaml => {
                let yd = yaml::parse_yaml_diagram(source)?;
                Ok(Diagram::Yaml(yd))
            }
            DiagramHint::Dot => {
                let block = common::extract_block(source).unwrap_or_default();
                let ds = dot::parse_dot_source(&block)?;
                Ok(Diagram::Dot(crate::model::dot::DotDiagram { source: ds }))
            }
            DiagramHint::Packet => {
                let pd = packet::parse_packet_diagram(source)?;
                Ok(Diagram::Packet(pd))
            }
            DiagramHint::Git => {
                let gd = git::parse_git_diagram(source)?;
                Ok(Diagram::Git(gd))
            }
            DiagramHint::Regex => {
                let rd = regex_diagram::parse_regex_diagram(source)?;
                Ok(Diagram::Regex(rd))
            }
            DiagramHint::Ebnf => {
                let ed = ebnf::parse_ebnf_diagram(source)?;
                Ok(Diagram::Ebnf(ed))
            }
            DiagramHint::Pie => {
                let pd = pie::parse_pie_diagram(source)?;
                Ok(Diagram::Pie(pd))
            }
            DiagramHint::Board => {
                let bd = board::parse_board_diagram(source)?;
                Ok(Diagram::Board(bd))
            }
            DiagramHint::Chronology => {
                let cd = chronology::parse_chronology_diagram(source)?;
                Ok(Diagram::Chronology(cd))
            }
            // v1.2026.3 renders @startproject as Gantt v2 (was: "not supported"
            // page on v1.2026.2). Reuse the gantt pipeline — Project shares the
            // task/lasts syntax (see temp/jar-probe/project.svg probe).
            DiagramHint::Project => {
                let gd = gantt::parse_gantt_diagram(source)?;
                Ok(Diagram::Gantt(gd))
            }
            DiagramHint::Hcl => {
                let hd = hcl::parse_hcl_diagram(source)?;
                Ok(Diagram::Hcl(hd))
            }
            DiagramHint::Wire => {
                let wd = wire::parse_wire_diagram(source)?;
                Ok(Diagram::Wire(wd))
            }
            DiagramHint::Math => {
                let md = math::parse_math_diagram(source)?;
                Ok(Diagram::Math(md))
            }
            DiagramHint::Latex => {
                let ld = math::parse_latex_diagram(source)?;
                Ok(Diagram::Latex(ld))
            }
            DiagramHint::Creole => {
                let cd = creole_diagram::parse_creole_diagram(source)?;
                Ok(Diagram::Creole(cd))
            }
            other => return Err(crate::Error::UnsupportedDiagram(format!("{other:?}"))),
        };
    }

    // For @startuml, use heuristic detection
    let content = common::extract_block(source);
    let body = content.as_deref().unwrap_or(source);
    let dtype = common::detect_diagram_type(body);

    match dtype {
        DiagramHint::Class => {
            let cd = class::parse_class_diagram_with_original(source, original_source)?;
            Ok(Diagram::Class(cd))
        }
        DiagramHint::Sequence => {
            let sd = sequence::parse_sequence_diagram_with_original(source, original_source)?;
            Ok(Diagram::Sequence(sd))
        }
        DiagramHint::Activity => {
            let ad = activity::parse_activity_diagram(source)?;
            Ok(Diagram::Activity(ad))
        }
        DiagramHint::State => {
            let sd = state::parse_state_diagram(source)?;
            Ok(Diagram::State(sd))
        }
        DiagramHint::UseCase => {
            let ud = usecase::parse_usecase_diagram(source)?;
            Ok(Diagram::UseCase(ud))
        }
        DiagramHint::Component => {
            let cd = component::parse_component_diagram(source)?;
            Ok(Diagram::Component(cd))
        }
        DiagramHint::Timing => {
            let td = timing::parse_timing_diagram(source)?;
            Ok(Diagram::Timing(td))
        }
        DiagramHint::Salt => {
            let sd = salt::parse_salt_diagram(source)?;
            Ok(Diagram::Salt(sd))
        }
        DiagramHint::Unknown(t) => {
            // Meta-only diagrams default to empty class diagram, matching Java
            // PlantUML which produces data-diagram-type="CLASS" for these.
            if !common::has_meaningful_uml_content(body) && !common::parse_meta(source).is_empty() {
                return Ok(Diagram::Class(ClassDiagram {
                    entities: Vec::new(),
                    links: Vec::new(),
                    groups: Vec::new(),
                    direction: Default::default(),
                    direction_explicit: false,
                    notes: Vec::new(),
                    hide_show_rules: Vec::new(),
                    stereotype_backgrounds: Default::default(),
                }));
            }
            Err(crate::Error::UnsupportedDiagram(t))
        }
        // These should be handled by start tag detection above
        DiagramHint::Bpm
        | DiagramHint::Def
        | DiagramHint::Chart
        | DiagramHint::Ditaa
        | DiagramHint::Erd
        | DiagramHint::Files
        | DiagramHint::Flow
        | DiagramHint::Gantt
        | DiagramHint::Jcckit
        | DiagramHint::Json
        | DiagramHint::Mindmap
        | DiagramHint::Nwdiag
        | DiagramHint::Wbs
        | DiagramHint::Yaml
        | DiagramHint::Dot
        | DiagramHint::Regex
        | DiagramHint::Ebnf
        | DiagramHint::Packet
        | DiagramHint::Git
        | DiagramHint::Pie
        | DiagramHint::Board
        | DiagramHint::Chronology
        | DiagramHint::Project
        | DiagramHint::Hcl
        | DiagramHint::Wire
        | DiagramHint::Math
        | DiagramHint::Latex
        | DiagramHint::Creole => Err(crate::Error::UnsupportedDiagram(format!("{dtype:?}"))),
    }
}

/// Internal diagram type hint
#[derive(Debug)]
pub enum DiagramHint {
    Bpm,
    Class,
    Def,
    Sequence,
    Activity,
    State,
    Component,
    Chart,
    Ditaa,
    Erd,
    Files,
    Flow,
    Gantt,
    Jcckit,
    Json,
    Mindmap,
    Nwdiag,
    Salt,
    Timing,
    Wbs,
    Yaml,
    Dot,
    UseCase,
    Packet,
    Git,
    Regex,
    Ebnf,
    Pie,
    Board,
    Chronology,
    Project,
    Hcl,
    Wire,
    Math,
    Latex,
    Creole,
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meta_only_uml_as_empty_class_diagram() {
        let src = "@startuml\ntitle\nOnly meta\nend title\n@enduml\n";
        let diagram = parse(src).expect("parse failed");
        match diagram {
            Diagram::Class(cd) => {
                assert!(cd.entities.is_empty());
                assert!(cd.links.is_empty());
                assert!(cd.notes.is_empty());
            }
            other => panic!("expected empty class fallback, got {:?}", other),
        }
    }
}
