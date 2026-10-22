//! Local opt-in wake recognition. A continuous session retains microphone ownership
//! until a phrase matches; all Windows speech calls stay on the worker thread.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
};
use std::thread::{self, JoinHandle};
use windows_collections::IIterable;
use windows_startup::Foundation::{TimeSpan, TypedEventHandler};
use windows_startup::Media::SpeechRecognition::{
    ISpeechRecognitionConstraint, SpeechContinuousRecognitionCompletedEventArgs,
    SpeechContinuousRecognitionResultGeneratedEventArgs, SpeechContinuousRecognitionSession,
    SpeechRecognitionConfidence, SpeechRecognitionListConstraint, SpeechRecognitionResultStatus,
    SpeechRecognizer,
};
use windows_startup::core::{HSTRING, Interface};
const MAX_PHRASE_CHARS: usize = 64;
const INITIAL_SILENCE_TICKS: i64 = 300_000_000;
const END_SILENCE_TICKS: i64 = 4_500_000;
const BABBLE_TICKS: i64 = 300_000_000;
#[derive(Clone, Debug)]
pub enum Event {
    Listening { phrase: String },
    Triggered { phrase: String },
    Off,
    Error { phrase: String, message: String },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    Low,
    Balanced,
    High,
}
impl Sensitivity {
    pub fn from_name(value: &str) -> Self {
        match value {
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Balanced,
        }
    }
}
enum Command {
    Arm(u64, String, Sensitivity),
    Off(u64),
    Matched(u64),
    Ended(u64, String),
    Shutdown,
}
pub struct WakeWordService {
    commands: Sender<Command>,
    events: Receiver<(u64, Event)>,
    revision: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}
impl WakeWordService {
    pub fn new(notify: impl Fn() + Send + 'static) -> Self {
        let (commands, rx) = mpsc::channel();
        let (tx, events) = mpsc::channel();
        let revision = Arc::new(AtomicU64::new(0));
        let callbacks = commands.clone();
        let generation = Arc::clone(&revision);
        let worker = thread::Builder::new()
            .name("island-wake-phrase".into())
            .spawn(move || run(rx, callbacks, tx, generation, Box::new(notify)))
            .ok();
        Self {
            commands,
            events,
            revision,
            worker,
        }
    }
    pub fn arm(&self, phrase: &str, sensitivity: Sensitivity) -> Result<String, String> {
        let phrase = validate_phrase(phrase)?;
        let id = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        self.commands
            .send(Command::Arm(id, phrase.clone(), sensitivity))
            .map_err(|_| "The local listener is unavailable. Restart the app.".to_owned())?;
        Ok(phrase)
    }
    pub fn disarm(&self) {
        let id = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.commands.send(Command::Off(id));
    }
    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        loop {
            let (id, event) = self.events.try_recv()?;
            if id == self.revision.load(Ordering::Acquire) {
                return Ok(event);
            }
        }
    }
}
impl Drop for WakeWordService {
    fn drop(&mut self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
pub fn validate_phrase(value: &str) -> Result<String, String> {
    let phrase = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let words = phrase.split_whitespace().count();
    if phrase.chars().count() > MAX_PHRASE_CHARS
        || !(2..=5).contains(&words)
        || phrase.chars().any(char::is_control)
    {
        return Err("Use a distinctive wake phrase with 2–5 words, up to 64 characters.".into());
    }
    Ok(phrase)
}

fn run(
    commands: Receiver<Command>,
    callbacks: Sender<Command>,
    events: Sender<(u64, Event)>,
    revision: Arc<AtomicU64>,
    notify: Box<dyn Fn() + Send>,
) {
    use windows_startup::Win32::System::WinRT::{
        RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
    };
    let mut initialized = false;
    let mut active: Option<SpeechRecognizer> = None;
    let mut desktop: Option<DesktopListener> = None;
    let mut active_id = 0;
    let mut phrase = String::new();
    let mut sensitivity = Sensitivity::Balanced;
    loop {
        let command = if desktop.is_some() {
            match commands.recv_timeout(std::time::Duration::from_millis(80)) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let result = desktop.as_ref().unwrap().matched(&phrase, sensitivity);
                    match result {
                        Ok(true) => {
                            desktop.take();
                            publish(
                                &events,
                                &notify,
                                active_id,
                                Event::Triggered {
                                    phrase: phrase.clone(),
                                },
                            );
                        }
                        Err(message) => {
                            desktop.take();
                            publish(
                                &events,
                                &notify,
                                active_id,
                                Event::Error {
                                    phrase: phrase.clone(),
                                    message,
                                },
                            );
                        }
                        Ok(false) => {}
                    }
                    continue;
                }
            }
        } else {
            match commands.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };
        match command {
            Command::Shutdown => break,
            Command::Off(id) => {
                desktop.take();
                close_recognizer(&mut active);
                publish(&events, &notify, id, Event::Off);
            }
            Command::Arm(id, next, next_sensitivity) => {
                if id != revision.load(Ordering::Acquire) {
                    continue;
                }
                desktop.take();
                close_recognizer(&mut active);
                phrase = next;
                sensitivity = next_sensitivity;
                if !initialized {
                    if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                        publish(
                            &events,
                            &notify,
                            id,
                            Event::Error {
                                phrase: phrase.clone(),
                                message: speech_error(error),
                            },
                        );
                        continue;
                    }
                    initialized = true;
                }
                // SAPI works in the standalone desktop executable and requires no
                // package identity or downloadable model. Keep WinRT as fallback.
                let desktop_error = match DesktopListener::new(&phrase) {
                    Ok(listener) => {
                        if id != revision.load(Ordering::Acquire) {
                            continue;
                        }
                        desktop = Some(listener);
                        active_id = id;
                        publish(
                            &events,
                            &notify,
                            id,
                            Event::Listening {
                                phrase: phrase.clone(),
                            },
                        );
                        continue;
                    }
                    Err(error) => error,
                };
                let started = (|| -> Result<SpeechRecognizer, String> {
                    let recognizer = create_recognizer(&phrase)?;
                    let result =
                        start_continuous(&recognizer, id, &phrase, sensitivity, callbacks.clone());
                    if let Err(error) = result {
                        let _ = recognizer.Close();
                        return Err(error);
                    }
                    Ok(recognizer)
                })();
                match started {
                    Ok(value) => {
                        active = Some(value);
                        if id != revision.load(Ordering::Acquire) {
                            close_recognizer(&mut active);
                            continue;
                        }
                        publish(
                            &events,
                            &notify,
                            id,
                            Event::Listening {
                                phrase: phrase.clone(),
                            },
                        );
                    }
                    Err(message) => publish(
                        &events,
                        &notify,
                        id,
                        Event::Error {
                            phrase: phrase.clone(),
                            message: format!("{desktop_error} {message}"),
                        },
                    ),
                }
            }
            Command::Matched(id) if id == revision.load(Ordering::Acquire) && active.is_some() => {
                close_recognizer(&mut active);
                publish(
                    &events,
                    &notify,
                    id,
                    Event::Triggered {
                        phrase: phrase.clone(),
                    },
                );
            }
            Command::Ended(id, message)
                if id == revision.load(Ordering::Acquire) && active.is_some() =>
            {
                close_recognizer(&mut active);
                publish(
                    &events,
                    &notify,
                    id,
                    Event::Error {
                        phrase: phrase.clone(),
                        message,
                    },
                );
            }
            _ => {}
        }
    }
    desktop.take();
    close_recognizer(&mut active);
    if initialized {
        unsafe { RoUninitialize() };
    }
}
fn start_continuous(
    recognizer: &SpeechRecognizer,
    id: u64,
    phrase: &str,
    sensitivity: Sensitivity,
    commands: Sender<Command>,
) -> Result<(), String> {
    let session = recognizer
        .ContinuousRecognitionSession()
        .map_err(speech_error)?;
    // Custom command grammars stay active; retain the Windows session default.
    let expected = phrase.to_owned();
    let matched = commands.clone();
    session
        .ResultGenerated(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionResultGeneratedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(result) = args.Result() {
                    if result
                        .Status()
                        .is_ok_and(|status| status == SpeechRecognitionResultStatus::Success)
                        && result.Confidence().is_ok_and(|confidence| {
                            confidence == SpeechRecognitionConfidence::High
                                || (sensitivity == Sensitivity::High
                                    && confidence == SpeechRecognitionConfidence::Medium)
                        })
                        && (sensitivity != Sensitivity::Low
                            || result
                                .RawConfidence()
                                .is_ok_and(|confidence| confidence >= 0.85))
                        && result
                            .Text()
                            .is_ok_and(|text| same_phrase(&text.to_string_lossy(), &expected))
                    {
                        let _ = matched.send(Command::Matched(id));
                    }
                }
            }
            Ok(())
        }))
        .map_err(speech_error)?;
    session
        .Completed(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionCompletedEventArgs,
        >::new(move |_, args| {
            let status = args.as_ref().and_then(|args| args.Status().ok());
            let _ = commands.send(Command::Ended(
                id,
                format!(
                    "Windows stopped listening ({status:?}). Check microphone access and try again."
                ),
            ));
            Ok(())
        }))
        .map_err(speech_error)?;
    session
        .StartAsync()
        .map_err(speech_error)?
        .join()
        .map_err(speech_error)
}
fn close_recognizer(recognizer: &mut Option<SpeechRecognizer>) {
    if let Some(recognizer) = recognizer.take() {
        if let Ok(session) = recognizer.ContinuousRecognitionSession() {
            if let Ok(operation) = session.CancelAsync() {
                let _ = operation.join();
            }
        }
        let _ = recognizer.Close();
    }
}
fn create_recognizer(phrase: &str) -> Result<SpeechRecognizer, String> {
    let recognizer = SpeechRecognizer::new().map_err(speech_error)?;
    let commands = IIterable::<HSTRING>::from(vec![HSTRING::from(phrase)]);
    let list = SpeechRecognitionListConstraint::Create(&commands).map_err(speech_error)?;
    let constraint: ISpeechRecognitionConstraint = list.cast().map_err(speech_error)?;
    recognizer
        .Constraints()
        .map_err(speech_error)?
        .Append(&constraint)
        .map_err(speech_error)?;
    let timeouts = recognizer.Timeouts().map_err(speech_error)?;
    timeouts
        .SetInitialSilenceTimeout(TimeSpan {
            Duration: INITIAL_SILENCE_TICKS,
        })
        .map_err(speech_error)?;
    timeouts
        .SetEndSilenceTimeout(TimeSpan {
            Duration: END_SILENCE_TICKS,
        })
        .map_err(speech_error)?;
    timeouts
        .SetBabbleTimeout(TimeSpan {
            Duration: BABBLE_TICKS,
        })
        .map_err(speech_error)?;
    let compilation = recognizer
        .CompileConstraintsAsync()
        .map_err(speech_error)?
        .join()
        .map_err(speech_error)?;
    if compilation.Status().map_err(speech_error)? != SpeechRecognitionResultStatus::Success {
        return Err("Windows could not compile this wake phrase. Check that a matching speech language pack is installed.".to_owned());
    }
    Ok(recognizer)
}

fn same_phrase(left: &str, right: &str) -> bool {
    let normalize = |value: &str| {
        value
            .chars()
            .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    normalize(left).split_whitespace().collect::<Vec<_>>()
        == normalize(right).split_whitespace().collect::<Vec<_>>()
}

fn speech_error(error: impl std::fmt::Display) -> String {
    let detail = error.to_string();
    if detail.contains("0x80045509") || detail.contains("0x8004550a") {
        "Windows speech recognition is unavailable. Install a speech language pack, then try again."
            .into()
    } else if detail.contains("0x80070005") {
        "Allow microphone access for Pluto in Windows Privacy settings, then try again.".into()
    } else if detail.contains("0x80040154") || detail.contains("0x80073D54") {
        "Local wake detection requires the installed Pluto release package. Run the MSIX build, then try again.".into()
    } else {
        format!(
            "Windows could not start local wake detection: {}",
            detail.chars().take(180).collect::<String>()
        )
    }
}

fn publish(events: &Sender<(u64, Event)>, notify: &dyn Fn(), generation: u64, event: Event) {
    let _ = events.send((generation, event));
    notify();
}

// All COM interfaces remain on the listener worker. Poll only while explicitly
// armed; releasing this object closes the microphone before dictation begins.
struct DesktopListener {
    recognizer: windows_startup::Win32::Media::Speech::ISpRecognizer,
    context: windows_startup::Win32::Media::Speech::ISpRecoContext,
    _grammar: windows_startup::Win32::Media::Speech::ISpRecoGrammar,
}
impl DesktopListener {
    fn new(phrase: &str) -> Result<Self, String> {
        use windows_startup::Win32::{Media::Speech::*, System::Com::*};
        use windows_startup::core::{PCWSTR, w};
        let create = || -> windows_startup::core::Result<Self> {
            unsafe {
                let recognizer: ISpRecognizer =
                    CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_INPROC_SERVER)?;
                let category: ISpObjectTokenCategory =
                    CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER)?;
                category.SetId(SPCAT_AUDIOIN, false)?;
                let audio: ISpObjectToken =
                    CoCreateInstance(&SpObjectToken, None, CLSCTX_INPROC_SERVER)?;
                let token_id = category.GetDefaultTokenId()?;
                let result = audio.SetId(PCWSTR::null(), PCWSTR(token_id.0), false);
                CoTaskMemFree(Some(token_id.0.cast()));
                result?;
                recognizer.SetInput(&audio, true)?;
                let context = recognizer.CreateRecoContext()?;
                let interest = (1u64 << SPEI_RECOGNITION.0) | (1u64 << 30) | (1u64 << 33);
                context.SetInterest(interest, interest)?;
                let grammar = context.CreateGrammar(1)?;
                let mut rule = SPSTATEHANDLE::default();
                grammar.GetRule(
                    w!("wake"),
                    1,
                    (SPRAF_TopLevel.0 | SPRAF_Dynamic.0) as u32,
                    true,
                    &mut rule,
                )?;
                let phrase = HSTRING::from(phrase);
                grammar.AddWordTransition(
                    rule,
                    SPSTATEHANDLE::default(),
                    &phrase,
                    w!(" "),
                    SPWT_LEXICAL,
                    1.0,
                    std::ptr::null(),
                )?;
                grammar.Commit(0)?;
                grammar.SetRuleState(w!("wake"), std::ptr::null_mut(), SPRS_ACTIVE)?;
                recognizer.SetRecoState(SPRST_ACTIVE_ALWAYS)?;
                Ok(Self {
                    recognizer,
                    context,
                    _grammar: grammar,
                })
            }
        };
        create().map_err(|error| format!("Desktop speech could not start. Check the installed Windows speech language and default microphone ({error})."))
    }
    fn matched(&self, expected: &str, sensitivity: Sensitivity) -> Result<bool, String> {
        use windows_startup::Win32::{Media::Speech::*, System::Com::CoTaskMemFree};
        use windows_startup::core::{IUnknown, PWSTR};
        let read = || -> windows_startup::core::Result<bool> {
            unsafe {
                // Bound work per tick even if the device produces a burst of events.
                for _ in 0..16 {
                    let mut event = SPEVENT::default();
                    let mut fetched = 0;
                    self.context.GetEvents(1, &mut event, &mut fetched)?;
                    if fetched == 0 {
                        break;
                    }
                    if (event._bitfield >> 16) != SPET_LPARAM_IS_OBJECT.0 || event.lParam.0 == 0 {
                        continue;
                    }
                    let object = IUnknown::from_raw(event.lParam.0 as *mut _);
                    if event._bitfield & 0xffff != SPEI_RECOGNITION.0 {
                        continue;
                    }
                    let result: ISpRecoResult = object.cast()?;
                    let mut text = PWSTR::null();
                    result.GetText(0, u32::MAX, true, &mut text, None)?;
                    let matches = same_phrase(&text.to_string().unwrap_or_default(), expected);
                    CoTaskMemFree(Some(text.0.cast()));
                    if matches {
                        let phrase = result.GetPhrase()?;
                        let rule = &(*phrase).Base.Rule;
                        let minimum = if sensitivity == Sensitivity::High {
                            0
                        } else {
                            1
                        };
                        let mut confident = rule.Confidence >= minimum;
                        // SAPI confidence is categorical (-1, 0, 1), not a probability.
                        // Strict mode also requires confident individual words.
                        if sensitivity == Sensitivity::Low {
                            confident &= rule.ulCountOfElements > 0
                                && rule.ulCountOfElements <= 32
                                && !(*phrase).Base.pElements.is_null();
                            if confident {
                                let words = std::slice::from_raw_parts(
                                    (*phrase).Base.pElements.add(rule.ulFirstElement as usize),
                                    rule.ulCountOfElements as usize,
                                );
                                confident &= words.iter().all(|word| word.ActualConfidence > 0);
                            }
                        }
                        CoTaskMemFree(Some(phrase.cast()));
                        if confident {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
        };
        read().map_err(speech_error)
    }
}
impl Drop for DesktopListener {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .recognizer
                .SetRecoState(windows_startup::Win32::Media::Speech::SPRST_INACTIVE_WITH_PURGE);
        }
    }
}
