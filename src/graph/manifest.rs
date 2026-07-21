use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub graph_id: String,
    pub node_schema: String,
    pub edge_schema: String,
    pub content_root: String,
    pub id_pattern: String,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema_version: 1,
            graph_id: "pulse-main".to_string(),
            node_schema: "schemas/node.schema.json".to_string(),
            edge_schema: "schemas/edge.schema.json".to_string(),
            content_root: "../../works".to_string(),
            id_pattern: "^(EP|ST|TK|DEC)-[0-9]{3,}$".to_string(),
        }
    }
}

pub const NODE_SCHEMA: &str = include_str!("../schema/node.schema.json");
pub const EDGE_SCHEMA: &str = include_str!("../schema/edge.schema.json");
