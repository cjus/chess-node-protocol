//! The voice domain (spec, "The voice domain"): text to speech and speech to text.
//!
//! | Method | Kind | Types |
//! | --- | --- | --- |
//! | `voice/voices` | request | [`Voices`]: no params → [`VoicesResult`] |
//! | `voice/speak` | request | [`Speak`]: [`SpeakParams`] → [`SpeakResult`] |
//! | `voice/transcribe` | request | [`Transcribe`]: [`TranscribeParams`] → [`TranscribeResult`] |
//! | `voice/models` | request | [`Models`]: no params → [`ModelsResult`] |
//! | `voice/modelState` | notification, node to client | [`ModelStateChanged`]: [`ModelStateParams`] |
//! | `voice/listen` | request | [`Listen`]: [`ListenParams`] → [`ListenResult`] |
//! | `voice/audio` | notification, client to node | [`Audio`]: [`AudioParams`] |
//! | `voice/speechStarted` | notification, node to client | [`SpeechStarted`]: [`SpeechStartedParams`] |
//! | `voice/heard` | notification, node to client | [`Heard`]: [`HeardParams`] |
//!
//! A voice node only turns text into audio and audio into text: it never touches a
//! microphone or a speaker. Audio travels as base64 ([RFC 4648], with padding) in one of the
//! [`AudioFormat`]s.
//!
//! ## Listening
//!
//! `voice/listen` (capability `listen`) hears speech as it comes, so a player can talk without
//! holding a button. The client streams its microphone's samples in `voice/audio`
//! notifications, and the node says when someone starts speaking (`voice/speechStarted`) and,
//! each time they pause, what they said (`voice/heard`). The listen is answered when the
//! client ends its audio (`end`), after the last `voice/heard`, or at once with `Cancelled`
//! when it is cancelled, dropping whatever it was hearing. Times in these messages are
//! milliseconds into the listen's audio, not clock times.
//!
//! **Pitfall: audio and answers cross.** The client may still be sending audio for a listen
//! the node has answered (an error, say). The node ignores audio for a listen it doesn't
//! have, and the client stops sending once it sees the answer.
//!
//! ## Hotwords
//!
//! A client that knows what is likely to be said gives it as `hotwords` (capability
//! `hotwords`), and the node favours those words where the sound allows them: "knight" over
//! "night". The client knows its domain, so the node needn't: a chess board sends the words
//! of a move, another program its own.
//!
//! A node downloads and loads its own models, and says how far it has got
//! with `voice/modelState`; until a model is ready, requests that need it are answered with
//! [`code::NOT_READY`](crate::code::NOT_READY).
//!
//! [RFC 4648]: https://www.rfc-editor.org/rfc/rfc4648#section-4

use serde::{Deserialize, Serialize};

use crate::message::Id;
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
    /// Hearing speech as it comes (`voice/listen`, and its notifications).
    #[serde(default)]
    pub listen: bool,
    /// Each word's start and end (`wordTimes`).
    #[serde(default)]
    pub word_times: bool,
    /// Words the client expects to be said, favoured in what is heard (`hotwords`).
    #[serde(default)]
    pub hotwords: bool,
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
    /// Words and phrases likely to be said, which the node favours (capability `hotwords`).
    /// Missing: none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotwords: Vec<String>,
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
    /// Hears when speech starts and ends, for `voice/listen`.
    Listen,
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

/// The `voice/listen` request: hear speech as it comes, until the audio ends (see the
/// [module docs](self)).
pub enum Listen {}

impl RequestMethod for Listen {
    const NAME: &'static str = "voice/listen";
    type Params = ListenParams;
    type Result = ListenResult;
}

/// How the audio will come, and how to hear it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenParams {
    /// The audio's format: [`AudioFormat::PcmF32`], the format that streams.
    pub format: AudioFormat,
    /// Samples a second. Every node takes 16000.
    pub sample_rate: u32,
    /// The language spoken, a BCP 47 tag. Missing: the node's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// How long a pause ends what someone is saying, in milliseconds. Missing: the node
    /// decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_silence_ms: Option<u64>,
    /// Shorter sounds aren't speech (a cough, a click), in milliseconds. Missing: the node
    /// decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speech_ms: Option<u64>,
    /// The longest one utterance may be, in milliseconds: at this length it ends, pause or
    /// not. Missing: the node decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speech_ms: Option<u64>,
    /// How much of the audio from before an utterance's start it is heard with, in
    /// milliseconds, so that a short first word said fast isn't cut off; 0 for none. Missing:
    /// the node decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_in_ms: Option<u64>,
    /// Whether `voice/heard` gives each word's start and end (capability `wordTimes`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub word_times: bool,
    /// Words and phrases likely to be said, for every utterance (capability `hotwords`).
    /// Missing: none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotwords: Vec<String>,
}

impl ListenParams {
    /// Raw samples at `sample_rate`, with every other setting the node's.
    pub fn new(sample_rate: u32) -> ListenParams {
        ListenParams {
            format: AudioFormat::PcmF32,
            sample_rate,
            language: None,
            end_silence_ms: None,
            min_speech_ms: None,
            max_speech_ms: None,
            lead_in_ms: None,
            word_times: false,
            hotwords: Vec::new(),
        }
    }
}

/// A listen's answer, once its audio has ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenResult {
    /// How much audio the node heard in all, in milliseconds.
    pub duration_ms: u64,
}

/// The `voice/audio` notification, from the client: the next piece of a listen's audio.
pub enum Audio {}

impl NotificationMethod for Audio {
    const NAME: &'static str = "voice/audio";
    type Params = AudioParams;
}

/// A piece of a listen's audio, or its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioParams {
    /// The `voice/listen` request it belongs to.
    pub id: Id,
    /// The next samples, base64, in the listen's format and rate. May be missing with `end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    /// Whether the audio has ended: no more comes for this listen.
    #[serde(default, skip_serializing_if = "is_false")]
    pub end: bool,
}

/// The `voice/speechStarted` notification, from the node: someone has started speaking.
pub enum SpeechStarted {}

impl NotificationMethod for SpeechStarted {
    const NAME: &'static str = "voice/speechStarted";
    type Params = SpeechStartedParams;
}

/// Where speech started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStartedParams {
    /// The `voice/listen` request it belongs to.
    pub id: Id,
    /// About where the speaker started, in milliseconds into the listen's audio.
    pub start_ms: u64,
}

/// The `voice/heard` notification, from the node: what someone said, once they paused.
pub enum Heard {}

impl NotificationMethod for Heard {
    const NAME: &'static str = "voice/heard";
    type Params = HeardParams;
}

/// What was said in one utterance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardParams {
    /// The `voice/listen` request it belongs to.
    pub id: Id,
    /// What was said. Empty if no words were made out.
    pub text: String,
    /// Where the speech starts, in milliseconds into the listen's audio.
    pub start_ms: u64,
    /// Where it ends.
    pub end_ms: u64,
    /// Each word's start and end, if asked for and the model can tell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordTime>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
