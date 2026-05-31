//! Native message and content block helper functions.

use serde_json::Value;
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicDocumentSource, AnthropicImageSource,
    AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::{
    GoogleFileData, GoogleFunctionResponse, GoogleInlineData, GooglePart,
};
use skald_spec::wire::openai_chat::{
    OpenAiContentPart, OpenAiFilePart, OpenAiImageUrl, OpenAiInputAudio,
};
use skald_spec::wire::openai_responses::OpenAiResponseContentPart;

/// Builds an `OpenAI` Chat text content part.
pub fn openai_text_part(text: impl Into<String>) -> OpenAiContentPart {
    OpenAiContentPart::Text { text: text.into() }
}

/// Builds an `OpenAI` Chat image URL content part.
pub fn openai_image_url_part(url: impl Into<String>, detail: Option<String>) -> OpenAiContentPart {
    OpenAiContentPart::ImageUrl {
        image_url: OpenAiImageUrl {
            url: url.into(),
            detail,
        },
    }
}

/// Builds an `OpenAI` Chat base64 audio content part.
pub fn openai_audio_part(data: impl Into<String>, format: impl Into<String>) -> OpenAiContentPart {
    OpenAiContentPart::InputAudio {
        input_audio: OpenAiInputAudio {
            data: data.into(),
            format: format.into(),
        },
    }
}

/// Builds an `OpenAI` Chat file-id content part.
pub fn openai_file_id_part(file_id: impl Into<String>) -> OpenAiContentPart {
    OpenAiContentPart::File {
        file: OpenAiFilePart {
            file_id: Some(file_id.into()),
            file_data: None,
            filename: None,
        },
    }
}

/// Builds an `OpenAI` Chat inline file content part.
pub fn openai_file_data_part(
    file_data: impl Into<String>,
    filename: Option<String>,
) -> OpenAiContentPart {
    OpenAiContentPart::File {
        file: OpenAiFilePart {
            file_id: None,
            file_data: Some(file_data.into()),
            filename,
        },
    }
}

/// Builds an `OpenAI` Responses input text part.
pub fn openai_responses_input_text(text: impl Into<String>) -> OpenAiResponseContentPart {
    OpenAiResponseContentPart::InputText { text: text.into() }
}

/// Builds an `OpenAI` Responses output text part.
pub fn openai_responses_output_text(text: impl Into<String>) -> OpenAiResponseContentPart {
    OpenAiResponseContentPart::OutputText { text: text.into() }
}

/// Builds an `OpenAI` Responses image URL part.
pub fn openai_responses_image_url(
    image_url: impl Into<String>,
    detail: Option<String>,
) -> OpenAiResponseContentPart {
    OpenAiResponseContentPart::InputImage {
        image_url: image_url.into(),
        detail,
    }
}

/// Builds an `OpenAI` Responses file-id part.
pub fn openai_responses_file_id(file_id: impl Into<String>) -> OpenAiResponseContentPart {
    OpenAiResponseContentPart::InputFile {
        file_id: file_id.into(),
    }
}

/// Builds an Anthropic text content block.
pub fn anthropic_text_block(text: impl Into<String>) -> AnthropicContentBlock {
    AnthropicContentBlock::Text {
        text: text.into(),
        cache_control: None,
        citations: None,
    }
}

/// Builds an Anthropic image-by-URL content block.
pub fn anthropic_image_url_block(url: impl Into<String>) -> AnthropicContentBlock {
    AnthropicContentBlock::Image {
        source: AnthropicImageSource::Url { url: url.into() },
        cache_control: None,
    }
}

/// Builds an Anthropic base64 image content block.
pub fn anthropic_image_base64_block(
    media_type: impl Into<String>,
    data: impl Into<String>,
) -> AnthropicContentBlock {
    AnthropicContentBlock::Image {
        source: AnthropicImageSource::Base64 {
            media_type: media_type.into(),
            data: data.into(),
        },
        cache_control: None,
    }
}

/// Builds an Anthropic image file-id content block.
pub fn anthropic_image_file_id_block(file_id: impl Into<String>) -> AnthropicContentBlock {
    AnthropicContentBlock::Image {
        source: AnthropicImageSource::FileId {
            file_id: file_id.into(),
        },
        cache_control: None,
    }
}

/// Builds an Anthropic text document content block.
pub fn anthropic_document_text_block(
    media_type: impl Into<String>,
    data: impl Into<String>,
    title: Option<String>,
) -> AnthropicContentBlock {
    AnthropicContentBlock::Document {
        source: AnthropicDocumentSource::Text {
            media_type: media_type.into(),
            data: data.into(),
        },
        cache_control: None,
        title,
        context: None,
        citations: None,
    }
}

/// Builds an Anthropic document file-id content block.
pub fn anthropic_document_file_id_block(
    file_id: impl Into<String>,
    title: Option<String>,
) -> AnthropicContentBlock {
    AnthropicContentBlock::Document {
        source: AnthropicDocumentSource::FileId {
            file_id: file_id.into(),
        },
        cache_control: None,
        title,
        context: None,
        citations: None,
    }
}

/// Builds an Anthropic tool-result content block.
pub fn anthropic_tool_result_block(
    tool_use_id: impl Into<String>,
    content: impl Into<String>,
    is_error: bool,
) -> AnthropicContentBlock {
    AnthropicContentBlock::ToolResult {
        tool_use_id: tool_use_id.into(),
        content: AnthropicToolResultContent::Text(content.into()),
        is_error: Some(is_error),
        cache_control: None,
    }
}

/// Builds a Google text part.
pub fn google_text_part(text: impl Into<String>) -> GooglePart {
    GooglePart::Text { text: text.into() }
}

/// Builds a Google inline-data part for base64 bytes.
pub fn google_inline_data_part(
    mime_type: impl Into<String>,
    data: impl Into<String>,
) -> GooglePart {
    GooglePart::InlineData {
        inline_data: GoogleInlineData {
            mime_type: mime_type.into(),
            data: data.into(),
        },
    }
}

/// Builds a Google file-data part.
pub fn google_file_data_part(
    mime_type: impl Into<String>,
    file_uri: impl Into<String>,
) -> GooglePart {
    GooglePart::FileData {
        file_data: GoogleFileData {
            mime_type: mime_type.into(),
            file_uri: file_uri.into(),
        },
    }
}

/// Builds a Google function-response part for tool results.
pub fn google_function_response_part(name: impl Into<String>, response: Value) -> GooglePart {
    GooglePart::FunctionResponse {
        function_response: GoogleFunctionResponse {
            name: name.into(),
            response,
        },
    }
}
