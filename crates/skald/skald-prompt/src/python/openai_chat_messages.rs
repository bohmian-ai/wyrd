//! `PyOpenAi*` message-side wrappers shared by request and response.
//!
//! Every message wrapper carries a `ChatMessageSource`, which lets the same
//! `PyOpenAiChatMessage` (and its part/tool-call wrappers) project either
//! from a request's `messages[i]` or from a response's
//! `choices[c].message`. The `.expect("guarded")` calls hold because the
//! constructors only run in those two contexts.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiContentPart, OpenAiFilePart, OpenAiImageUrl, OpenAiInputAudio,
    OpenAiMessageAnnotation, OpenAiMessageAudio, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall, OpenAiUrlCitation,
};
use wyrd_interfaces::error::CardPyResult;

use super::shared::ChatMessageSource;
use crate::prompt::wrong_variant;

// ──────────────────────────────────────────────────────────────────────────────
// OpenAI Chat — shared message types (Rule 5)
// ──────────────────────────────────────────────────────────────────────────────

#[pyclass(module = "wyrd.prompt", name = "OpenAiChatMessage")]
pub struct PyOpenAiChatMessage {
    pub(crate) src: ChatMessageSource,
}
impl PyOpenAiChatMessage {
    fn m(&self) -> &OpenAiChatMessage {
        self.src.msg()
    }
}
#[pymethods]
impl PyOpenAiChatMessage {
    #[getter]
    fn role(&self) -> &str {
        &self.m().role
    }
    #[getter]
    fn content(&self) -> Option<PyOpenAiMessageContent> {
        self.m().content.as_ref().map(|_| PyOpenAiMessageContent {
            src: self.src.clone(),
        })
    }
    #[getter]
    fn name(&self) -> Option<&str> {
        self.m().name.as_deref()
    }
    #[getter]
    fn tool_calls(&self) -> Option<Vec<PyOpenAiToolCall>> {
        self.m().tool_calls.as_ref().map(|tc| {
            (0..tc.len())
                .map(|i| PyOpenAiToolCall {
                    src: self.src.clone(),
                    call_index: i,
                })
                .collect()
        })
    }
    #[getter]
    fn tool_call_id(&self) -> Option<&str> {
        self.m().tool_call_id.as_deref()
    }
    #[getter]
    fn refusal(&self) -> Option<&str> {
        self.m().refusal.as_deref()
    }
    #[getter]
    fn annotations(&self) -> Vec<PyOpenAiMessageAnnotation> {
        (0..self.m().annotations.len())
            .map(|i| PyOpenAiMessageAnnotation {
                src: self.src.clone(),
                index: i,
            })
            .collect()
    }
    #[getter]
    fn audio(&self) -> Option<PyOpenAiMessageAudio> {
        self.m().audio.as_ref().map(|_| PyOpenAiMessageAudio {
            src: self.src.clone(),
        })
    }
    fn __repr__(&self) -> String {
        format!("OpenAiChatMessage(role={:?})", self.m().role)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageContent")]
pub struct PyOpenAiMessageContent {
    src: ChatMessageSource,
}
impl PyOpenAiMessageContent {
    fn c(&self) -> &OpenAiMessageContent {
        self.src.msg().content.as_ref().expect("guarded")
    }
}
#[pymethods]
impl PyOpenAiMessageContent {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.c() {
            OpenAiMessageContent::Text(_) => "text",
            OpenAiMessageContent::Parts(_) => "parts",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.c() {
            OpenAiMessageContent::Text(s) => Ok(s.clone()),
            OpenAiMessageContent::Parts(_) => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_parts(&self) -> CardPyResult<Vec<PyOpenAiContentPart>> {
        match self.c() {
            OpenAiMessageContent::Parts(ps) => Ok((0..ps.len())
                .map(|i| PyOpenAiContentPart {
                    src: self.src.clone(),
                    part_index: i,
                })
                .collect()),
            OpenAiMessageContent::Text(_) => Err(wrong_variant("parts", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageContent(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiContentPart")]
pub struct PyOpenAiContentPart {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiContentPart {
    fn p(&self) -> &OpenAiContentPart {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => &ps[self.part_index],
            OpenAiMessageContent::Text(_) => unreachable!("guarded by parent"),
        }
    }
}
#[pymethods]
impl PyOpenAiContentPart {
    #[getter]
    fn kind(&self) -> &'static str {
        match self.p() {
            OpenAiContentPart::Text { .. } => "text",
            OpenAiContentPart::ImageUrl { .. } => "image_url",
            OpenAiContentPart::InputAudio { .. } => "input_audio",
            OpenAiContentPart::File { .. } => "file",
        }
    }
    fn as_text(&self) -> CardPyResult<String> {
        match self.p() {
            OpenAiContentPart::Text { text } => Ok(text.clone()),
            _ => Err(wrong_variant("text", self.kind()).into()),
        }
    }
    fn as_image_url(&self) -> CardPyResult<PyOpenAiImageUrl> {
        match self.p() {
            OpenAiContentPart::ImageUrl { .. } => Ok(PyOpenAiImageUrl {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("image_url", self.kind()).into()),
        }
    }
    fn as_input_audio(&self) -> CardPyResult<PyOpenAiInputAudio> {
        match self.p() {
            OpenAiContentPart::InputAudio { .. } => Ok(PyOpenAiInputAudio {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("input_audio", self.kind()).into()),
        }
    }
    fn as_file(&self) -> CardPyResult<PyOpenAiFilePart> {
        match self.p() {
            OpenAiContentPart::File { .. } => Ok(PyOpenAiFilePart {
                src: self.src.clone(),
                part_index: self.part_index,
            }),
            _ => Err(wrong_variant("file", self.kind()).into()),
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiContentPart(kind={:?})", self.kind())
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiImageUrl")]
pub struct PyOpenAiImageUrl {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiImageUrl {
    fn i(&self) -> &OpenAiImageUrl {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::ImageUrl { image_url } => image_url,
                _ => unreachable!(),
            },
            OpenAiMessageContent::Text(_) => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiImageUrl {
    #[getter]
    fn url(&self) -> &str {
        &self.i().url
    }
    #[getter]
    fn detail(&self) -> Option<&str> {
        self.i().detail.as_deref()
    }
    fn __repr__(&self) -> String {
        "OpenAiImageUrl".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiInputAudio")]
pub struct PyOpenAiInputAudio {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiInputAudio {
    fn a(&self) -> &OpenAiInputAudio {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::InputAudio { input_audio } => input_audio,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiInputAudio {
    #[getter]
    fn data(&self) -> &str {
        &self.a().data
    }
    #[getter]
    fn format(&self) -> &str {
        &self.a().format
    }
    fn __repr__(&self) -> String {
        "OpenAiInputAudio".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiFilePart")]
pub struct PyOpenAiFilePart {
    src: ChatMessageSource,
    part_index: usize,
}
impl PyOpenAiFilePart {
    fn f(&self) -> &OpenAiFilePart {
        match self.src.msg().content.as_ref().expect("guarded") {
            OpenAiMessageContent::Parts(ps) => match &ps[self.part_index] {
                OpenAiContentPart::File { file } => file,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
#[pymethods]
impl PyOpenAiFilePart {
    #[getter]
    fn file_id(&self) -> Option<&str> {
        self.f().file_id.as_deref()
    }
    #[getter]
    fn file_data(&self) -> Option<&str> {
        self.f().file_data.as_deref()
    }
    #[getter]
    fn filename(&self) -> Option<&str> {
        self.f().filename.as_deref()
    }
    fn __repr__(&self) -> String {
        "OpenAiFilePart".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiToolCall")]
pub struct PyOpenAiToolCall {
    src: ChatMessageSource,
    call_index: usize,
}
impl PyOpenAiToolCall {
    fn c(&self) -> &OpenAiToolCall {
        &self.src.msg().tool_calls.as_ref().expect("guarded")[self.call_index]
    }
}
#[pymethods]
impl PyOpenAiToolCall {
    #[getter]
    fn id(&self) -> &str {
        &self.c().id
    }
    #[getter]
    fn kind(&self) -> &str {
        &self.c().kind
    }
    #[getter]
    fn function(&self) -> PyOpenAiToolFunctionCall {
        PyOpenAiToolFunctionCall {
            src: self.src.clone(),
            call_index: self.call_index,
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiToolCall(id={:?})", self.c().id)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiToolFunctionCall")]
pub struct PyOpenAiToolFunctionCall {
    src: ChatMessageSource,
    call_index: usize,
}
impl PyOpenAiToolFunctionCall {
    fn f(&self) -> &OpenAiToolFunctionCall {
        &self.src.msg().tool_calls.as_ref().expect("guarded")[self.call_index].function
    }
}
#[pymethods]
impl PyOpenAiToolFunctionCall {
    #[getter]
    fn name(&self) -> &str {
        &self.f().name
    }
    #[getter]
    fn arguments(&self) -> &str {
        &self.f().arguments
    }
    fn __repr__(&self) -> String {
        format!("OpenAiToolFunctionCall(name={:?})", self.f().name)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageAnnotation")]
pub struct PyOpenAiMessageAnnotation {
    src: ChatMessageSource,
    index: usize,
}
impl PyOpenAiMessageAnnotation {
    fn a(&self) -> &OpenAiMessageAnnotation {
        &self.src.msg().annotations[self.index]
    }
}
#[pymethods]
impl PyOpenAiMessageAnnotation {
    #[getter]
    fn kind(&self) -> &str {
        &self.a().kind
    }
    #[getter]
    fn url_citation(&self) -> PyOpenAiUrlCitation {
        PyOpenAiUrlCitation {
            src: self.src.clone(),
            index: self.index,
        }
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageAnnotation(kind={:?})", self.a().kind)
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiUrlCitation")]
pub struct PyOpenAiUrlCitation {
    src: ChatMessageSource,
    index: usize,
}
impl PyOpenAiUrlCitation {
    fn u(&self) -> &OpenAiUrlCitation {
        &self.src.msg().annotations[self.index].url_citation
    }
}
#[pymethods]
impl PyOpenAiUrlCitation {
    #[getter]
    fn url(&self) -> &str {
        &self.u().url
    }
    #[getter]
    fn title(&self) -> &str {
        &self.u().title
    }
    #[getter]
    fn start_index(&self) -> u32 {
        self.u().start_index
    }
    #[getter]
    fn end_index(&self) -> u32 {
        self.u().end_index
    }
    fn __repr__(&self) -> String {
        "OpenAiUrlCitation".to_owned()
    }
}

#[pyclass(module = "wyrd.prompt", name = "OpenAiMessageAudio")]
pub struct PyOpenAiMessageAudio {
    src: ChatMessageSource,
}
impl PyOpenAiMessageAudio {
    fn a(&self) -> &OpenAiMessageAudio {
        self.src.msg().audio.as_ref().expect("guarded")
    }
}
#[pymethods]
impl PyOpenAiMessageAudio {
    #[getter]
    fn id(&self) -> &str {
        &self.a().id
    }
    #[getter]
    fn expires_at(&self) -> u64 {
        self.a().expires_at
    }
    #[getter]
    fn data(&self) -> &str {
        &self.a().data
    }
    #[getter]
    fn transcript(&self) -> &str {
        &self.a().transcript
    }
    fn __repr__(&self) -> String {
        format!("OpenAiMessageAudio(id={:?})", self.a().id)
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyOpenAiChatMessage>()?;
    module.add_class::<PyOpenAiMessageContent>()?;
    module.add_class::<PyOpenAiContentPart>()?;
    module.add_class::<PyOpenAiImageUrl>()?;
    module.add_class::<PyOpenAiInputAudio>()?;
    module.add_class::<PyOpenAiFilePart>()?;
    module.add_class::<PyOpenAiToolCall>()?;
    module.add_class::<PyOpenAiToolFunctionCall>()?;
    module.add_class::<PyOpenAiMessageAnnotation>()?;
    module.add_class::<PyOpenAiUrlCitation>()?;
    module.add_class::<PyOpenAiMessageAudio>()?;
    Ok(())
}
