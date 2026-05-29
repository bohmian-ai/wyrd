use serde::{Deserialize, Serialize};

/// Anthropic citation block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicCitationV1 {
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: Option<String>,
        start_char_index: u32,
        end_char_index: u32,
    },
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: Option<String>,
        start_page_number: u32,
        end_page_number: u32,
    },
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: Option<String>,
        start_block_index: u32,
        end_block_index: u32,
    },
    WebSearchResultLocation {
        cited_text: String,
        title: Option<String>,
        url: String,
        encrypted_index: Option<String>,
    },
    SearchResultLocation {
        cited_text: String,
        source: String,
        title: Option<String>,
        start_block_index: u32,
        end_block_index: u32,
        search_result_index: u32,
    },
}
