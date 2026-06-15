/// Network diagram (nwdiag) IR.

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct NwdiagDiagram {
    pub networks: Vec<Network>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Network {
    pub name: String,
    pub address: Option<String>,
    pub color: Option<String>,
    pub servers: Vec<ServerRef>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ServerRef {
    pub name: String,
    pub address: Option<String>,
    pub description: Option<String>,
}
