//! The voice domain (spec, "The voice domain"): text to speech and speech to text.
//!
//! | Method | Kind | Types |
//! | --- | --- | --- |
//! | `voice/voices` | request | [`Voices`]: no params → [`VoicesResult`] |
//! | `voice/speak` | request | [`Speak`]: [`SpeakParams`] → [`SpeakResult`] |
//! | `voice/transcribe` | request | [`Transcribe`]: [`TranscribeParams`] → [`TranscribeResult`] |
//! | `voice/models` | request | [`Models`]: no params → [`ModelsResult`] |
//! | `voice/modelState` | notification, node to client | [`ModelStateChanged`]: [`ModelStateParams`] |
//!
//! A voice node only turns text into audio and audio into text: it never touches a
//! microphone or a speaker. Audio travels as base64 ([RFC 4648], with padding) in one of the
//! [`AudioFormat`]s. A node downloads and loads its own models, and says how far it has got
//! with `voice/modelState`; until a model is ready, requests that need it are answered with
//! [`code::NOT_READY`](crate::code::NOT_READY).
//!
//! [RFC 4648]: https://www.rfc-editor.org/rfc/rfc4648#section-4

use serde::{Deserialize, Serialize};

use crate::method::{NoParams, NotificationMethod, RequestMethod};

/// What a voice node can do. A missing capability is `false`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCapabilities {
    /// Text to speech (`voice/speak`).
    #[serde(default)]
    pub speak: bool,
    /// Speech to text (`voice/transcribe`).
    #[serde(default)]
    pub transcribe: bool,
    /// Each word's start and end (`wordTimes`).
    #[serde(default)]
    pub word_times: bool,
    /// The audio formats it reads and writes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<AudioFormat>,
}

/// How audio is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioFormat {
    /// A RIFF WAVE file: mono, 16-bit integer or 32-bit float samples.
    #[serde(rename = "wav")]
    Wav,
    /// Raw mono samples, 32-bit float, little-endian, at the stated sample rate.
    #[serde(rename = "pcmF32")]
    PcmF32,
}

/// The `voice/voices` request: the voices the node can speak with.
pub enum Voices {}

impl RequestMethod for Voices {
    const NAME: &'static str = "voice/voices";
    type Params = NoParams;
    type Result = VoicesResult;
}

/// The voices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicesResult {
    /// Every voice. The first is the default.
    pub voices: Vec<Voice>,
}

/// A voice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    /// What `voice/speak` calls it.
    pub id: String,
    /// Its name for people.
    pub name: String,
    /// Its language, a BCP 47 tag like `en-US`.
    pub language: String,
    /// More about it, like `male, warm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The `voice/speak` request: text to speech.
pub enum Speak {}

impl RequestMethod for Speak {
    const NAME: &'static str = "voice/speak";
    type Params = SpeakParams;
    type Result = SpeakResult;
}

/// What to say, and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakParams {
    /// The text, in plain words: no markup.
    pub text: String,
    /// The voice's id. Missing: the node's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// How fast, 1.0 being the voice's own pace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    /// The audio format wanted. Missing: `wav`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<AudioFormat>,
    /// Whether to return each word's start and end (capability `wordTimes`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub word_times: bool,
}

/// The speech.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakResult {
    /// The audio's format.
    pub format: AudioFormat,
    /// Samples a second.
    pub sample_rate: u32,
    /// The audio, base64.
    pub audio: String,
    /// Its length, in milliseconds.
    pub duration_ms: u64,
    /// Each word's start and end, if asked for and the model can tell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordTime>,
}

/// When a word is spoken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordTime {
    /// The word.
    pub text: String,
    /// Where it starts in the audio, in milliseconds.
    pub start_ms: u64,
    /// Where it ends.
    pub end_ms: u64,
}

/// The `voice/transcribe` request: speech to text.
pub enum Transcribe {}

impl RequestMethod for Transcribe {
    const NAME: &'static str = "voice/transcribe";
    type Params = TranscribeParams;
    type Result = TranscribeResult;
}

/// The speech to read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeParams {
    /// The audio, base64.
    pub audio: String,
    /// Its format.
    pub format: AudioFormat,
    /// Its sample rate, for `pcmF32` (a WAV file says its own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// The language spoken, a BCP 47 tag. Missing: the node's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Whether to return each word's start and end (capability `wordTimes`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub word_times: bool,
}

/// The text heard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeResult {
    /// What was said. Empty if nothing was.
    pub text: String,
    /// Each word's start and end, if asked for and the model can tell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordTime>,
}

/// The `voice/models` request: the models the node uses, and how ready each is.
pub enum Models {}

impl RequestMethod for Models {
    const NAME: &'static str = "voice/models";
    type Params = NoParams;
    type Result = ModelsResult;
}

/// The models.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResult {
    /// Every model.
    pub models: Vec<Model>,
}

/// A model the node uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// What `voice/modelState` calls it.
    pub id: String,
    /// Its name for people, like `Kokoro 82M`.
    pub name: String,
    /// What it does.
    pub purpose: ModelPurpose,
    /// Its licence, as an SPDX expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// How ready it is.
    pub state: ModelState,
}

/// What a model does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelPurpose {
    /// Text to speech.
    Speak,
    /// Speech to text.
    Transcribe,
}

/// How ready a model is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelState {
    /// Where it has got to.
    pub stage: Stage,
    /// While downloading: bytes so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// While downloading: bytes in all, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<u64>,
    /// Why it failed, for people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The stages a model goes through, in order. `ready` and `failed` are the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Not started.
    Waiting,
    /// Being downloaded.
    Downloading,
    /// Being unpacked.
    Unpacking,
    /// Being loaded into memory.
    Loading,
    /// Ready to use.
    Ready,
    /// It can't be used; `message` says why.
    Failed,
}

/// The `voice/modelState` notification, from the node: a model's state changed. Sent at each
/// new stage, and while downloading at most about once a second.
pub enum ModelStateChanged {}

impl NotificationMethod for ModelStateChanged {
    const NAME: &'static str = "voice/modelState";
    type Params = ModelStateParams;
}

/// A model's new state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStateParams {
    /// The model's id, as `voice/models` gives it.
    pub id: String,
    /// Its state.
    pub state: ModelState,
}

fn is_false(b: &bool) -> bool {
    !*b
}
