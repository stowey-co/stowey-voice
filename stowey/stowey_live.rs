//! Stowey Voice: decode the speech while the user is still talking.
//!
//! Upstream cuts a recording into chunks only once it runs past 40 seconds,
//! because its chunking exists to protect accuracy on long audio. A normal
//! dictation of 10 to 30 seconds is therefore decoded in one piece after the
//! key is released, and that decode is most of the wait.
//!
//! This chunker cuts a live recording at every real pause instead. Each chunk
//! is handed to the speech model the moment its pause is certain, so by the
//! time the key is released only the words since the last pause are left.
//!
//! `stowey/apply.py` swaps it in for upstream's chunker in
//! `managers/transcription/chunked.rs`. A whole clip, such as a file or a
//! history entry transcribed again, still goes through upstream's chunker
//! untouched: see [`Chunker::whole_clip`].

use crate::audio_toolkit::chunking::{join_transcripts, Chunker as LongFormChunker, MakeScorer};
use crate::audio_toolkit::vad::SpeechScorer;

const RATE: usize = 16_000;

/// At or above this a frame is speech, below `SILENCE` it is not, and in
/// between it stays what the frame before it was. Silero's own thresholds,
/// the ones upstream's chunker uses.
const SPEECH: f32 = 0.5;
const SILENCE: f32 = 0.35;

/// A pause this long ends a chunk. The recorder keeps 450 ms of silence after
/// speech and drops the rest, so a pause can only be watched for that long
/// while it happens. 350 ms leaves a margin, and is longer than the gaps
/// inside a phrase.
const PAUSE_SAMPLES: usize = 350 * RATE / 1000;
/// A chunk is never shorter than this, nor cut with less speech than this in
/// it: the speech model needs something to work with.
const MIN_CHUNK_SAMPLES: usize = 3 * RATE;
const MIN_SPEECH_SAMPLES: usize = 3 * RATE / 2;
/// Speech that runs this long without a real pause is cut at the best short
/// gap in it, so the wait at the end stays bounded for a fast talker too.
const LONG_CHUNK_SAMPLES: usize = 12 * RATE;
const SHORT_GAP_SAMPLES: usize = 90 * RATE / 1000;
/// With no gap at all, a hard cut. Upstream's own limit is 40 seconds.
const HARD_CHUNK_SAMPLES: usize = 30 * RATE;
/// Silence left over after the last cut is not worth a run of the speech
/// model, which may answer silence with words of its own.
const MAX_SILENT_TAIL_SAMPLES: usize = RATE;

/// Cuts audio into chunks as it arrives. Same shape as upstream's chunker, so
/// the decoder that owns it does not change.
pub struct Chunker {
    mode: Mode,
}

enum Mode {
    /// Nothing pushed yet.
    New(MakeScorer),
    /// A whole clip: upstream's chunker, unchanged.
    Clip(LongFormChunker),
    /// A live recording.
    Live(Live),
    /// Only while `mode` is being replaced.
    Empty,
}

impl Chunker {
    pub fn new(make_scorer: MakeScorer) -> Self {
        Self {
            mode: Mode::New(make_scorer),
        }
    }

    /// Marks the audio as one finished clip, to be cut the way upstream cuts
    /// it. Call before the first push; later it does nothing.
    pub fn whole_clip(&mut self) {
        self.mode = match std::mem::replace(&mut self.mode, Mode::Empty) {
            Mode::New(make_scorer) => Mode::Clip(LongFormChunker::new(make_scorer)),
            other => other,
        };
    }

    /// Samples pushed so far, chunked or not.
    pub fn samples(&self) -> usize {
        match &self.mode {
            Mode::New(_) | Mode::Empty => 0,
            Mode::Clip(chunker) => chunker.samples(),
            Mode::Live(live) => live.len,
        }
    }

    /// Adds audio. Returns the chunks this made certain, in order.
    pub fn push(&mut self, pcm: impl Into<Vec<f32>>) -> Vec<Vec<f32>> {
        if matches!(self.mode, Mode::New(_)) {
            self.mode = match std::mem::replace(&mut self.mode, Mode::Empty) {
                Mode::New(make_scorer) => Mode::Live(Live::new(make_scorer)),
                other => other,
            };
        }
        match &mut self.mode {
            Mode::Clip(chunker) => chunker.push(pcm),
            Mode::Live(live) => live.push(pcm.into()),
            Mode::New(_) | Mode::Empty => Vec::new(),
        }
    }

    /// Returns what is left, in order. Nothing more is pushed after.
    pub fn finish(&mut self) -> Vec<Vec<f32>> {
        match &mut self.mode {
            Mode::Clip(chunker) => chunker.finish(),
            Mode::Live(live) => live.finish(),
            Mode::New(_) | Mode::Empty => Vec::new(),
        }
    }
}

struct Live {
    make_scorer: MakeScorer,
    /// Made once there is enough audio for a first cut, so a short dictation
    /// never loads a second voice detector.
    scorer: Option<Box<dyn SpeechScorer>>,
    frame: usize,
    /// Set when scoring fails: nothing more is cut and the rest is one chunk.
    uncut: bool,
    /// Audio from `start` on; everything before it is already in a chunk.
    pending: Vec<f32>,
    /// Whether each scored whole frame of `pending` is speech.
    speech: Vec<bool>,
    /// The last scored frame's state, carried into the next one.
    in_speech: bool,
    start: usize,
    len: usize,
    cuts: usize,
}

impl Live {
    fn new(make_scorer: MakeScorer) -> Self {
        Self {
            make_scorer,
            scorer: None,
            frame: 0,
            uncut: false,
            pending: Vec::new(),
            speech: Vec::new(),
            in_speech: false,
            start: 0,
            len: 0,
            cuts: 0,
        }
    }

    fn push(&mut self, pcm: Vec<f32>) -> Vec<Vec<f32>> {
        self.len += pcm.len();
        if self.pending.is_empty() {
            self.pending = pcm;
        } else {
            self.pending.extend_from_slice(&pcm);
        }
        // No cut is possible in a chunk this short, so nothing needs scoring.
        if self.uncut || self.pending.len() < MIN_CHUNK_SAMPLES {
            return Vec::new();
        }
        if let Err(reason) = self.score() {
            log::warn!(
                "Stowey Voice: live chunking stopped at {:.1}s, the rest is decoded in one piece: {}",
                self.len as f64 / RATE as f64,
                reason
            );
            self.uncut = true;
            return Vec::new();
        }
        let mut chunks = Vec::new();
        while let Some(cut) = self.next_cut() {
            chunks.push(self.take(cut));
        }
        chunks
    }

    fn finish(&mut self) -> Vec<Vec<f32>> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        // What is left after a cut is often just the silence the recorder
        // kept behind the last word. It is counted as seen and not decoded.
        let silent = self.cuts > 0
            && !self.uncut
            && self.pending.len() <= MAX_SILENT_TAIL_SAMPLES
            && self.score().is_ok()
            && self.speech.iter().all(|speech| !speech);
        if silent {
            log::debug!(
                "Stowey Voice: live chunk tail of {:.2}s is silence, not decoded",
                self.pending.len() as f64 / RATE as f64
            );
            self.start = self.len;
            self.pending.clear();
            self.speech.clear();
            return Vec::new();
        }
        let rest = self.pending.len();
        vec![self.take(rest)]
    }

    /// Scores every whole frame of `pending` not scored yet.
    fn score(&mut self) -> Result<(), String> {
        if self.scorer.is_none() {
            let scorer = (self.make_scorer)().map_err(|e| format!("{e:#}"))?;
            self.frame = scorer.frame_samples();
            if self.frame == 0 {
                return Err("the voice detector reports an empty frame".to_string());
            }
            self.scorer = Some(scorer);
        }
        let Some(scorer) = self.scorer.as_mut() else {
            return Err("no voice detector".to_string());
        };
        let scored = self.speech.len() * self.frame;
        let whole = (self.pending.len() - scored) / self.frame * self.frame;
        if whole == 0 {
            return Ok(());
        }
        let probabilities = scorer
            .score(&self.pending[scored..scored + whole])
            .map_err(|e| format!("{e:#}"))?;
        for probability in probabilities {
            if probability >= SPEECH {
                self.in_speech = true;
            } else if probability < SILENCE {
                self.in_speech = false;
            }
            self.speech.push(self.in_speech);
        }
        Ok(())
    }

    /// Where the current chunk ends, in samples from its start, if that is
    /// certain yet.
    fn next_cut(&self) -> Option<usize> {
        let frame = self.frame;
        let frames = |samples: usize| samples.div_ceil(frame);
        let pause = frames(PAUSE_SAMPLES);
        let min_chunk = frames(MIN_CHUNK_SAMPLES);
        let min_speech = frames(MIN_SPEECH_SAMPLES);

        // A real pause: the first point where enough silence follows enough
        // speech. The cut keeps the pause with the words before it.
        let mut spoken = 0;
        let mut quiet = 0;
        for (index, &speech) in self.speech.iter().enumerate() {
            if speech {
                spoken += 1;
                quiet = 0;
                continue;
            }
            quiet += 1;
            if quiet >= pause && spoken >= min_speech && index + 1 >= min_chunk {
                return Some((index + 1) * frame);
            }
        }

        if self.speech.len() < frames(LONG_CHUNK_SAMPLES) {
            return None;
        }
        // Long speech without one: the longest short gap that has ended,
        // the later one on a tie, cut in its middle.
        let short_gap = frames(SHORT_GAP_SAMPLES);
        let mut best: Option<(usize, usize)> = None;
        let mut spoken = 0;
        let mut gap_start: Option<usize> = None;
        for (index, &speech) in self.speech.iter().enumerate() {
            match (speech, gap_start) {
                (false, None) => gap_start = Some(index),
                (true, Some(from)) => {
                    let length = index - from;
                    let middle = from + length / 2;
                    if length >= short_gap
                        && spoken >= min_speech
                        && middle >= min_chunk
                        && best.is_none_or(|(_, longest)| length >= longest)
                    {
                        best = Some((middle, length));
                    }
                    gap_start = None;
                }
                _ => {}
            }
            if speech {
                spoken += 1;
            }
        }
        if let Some((middle, _)) = best {
            return Some(middle * frame);
        }
        (self.speech.len() >= frames(HARD_CHUNK_SAMPLES)).then(|| self.speech.len() * frame)
    }

    fn take(&mut self, cut: usize) -> Vec<f32> {
        log::debug!(
            "Stowey Voice: live chunk {:.1}s to {:.1}s",
            self.start as f64 / RATE as f64,
            (self.start + cut) as f64 / RATE as f64
        );
        let chunk: Vec<f32> = self.pending.drain(..cut).collect();
        if self.frame > 0 {
            let scored = (cut / self.frame).min(self.speech.len());
            self.speech.drain(..scored);
        }
        self.start += cut;
        self.cuts += 1;
        chunk
    }
}

// ---------------------------------------------------------------------------
// Joining the chunks' texts
// ---------------------------------------------------------------------------

/// Words a sentence does not end on. A chunk that ends on one was cut in the
/// middle of a sentence, whatever full stop the speech model put after it.
const NEVER_LAST: [&str; 44] = [
    "the", "a", "an", "to", "of", "and", "or", "but", "with", "for", "from", "by", "as", "if",
    "when", "because", "which", "who", "my", "your", "our", "their", "his", "its", "is", "are",
    "was", "were", "am", "be", "will", "would", "can", "could", "should", "have", "has", "had",
    "into", "about", "than", "very", "really", "that",
];

/// Words that carry a sentence on when a chunk begins with them.
const CARRIES_ON: [&str; 6] = ["and", "but", "or", "because", "which", "so"];

/// Everyday English words that only carry a capital at the start of a
/// sentence, so they can lose it when the sentence turns out not to start
/// there. Anything not listed keeps its capital: it may be a name, and a
/// pause is exactly where a speaker reaches for one.
const PLAIN_WORDS: &str = "the a an to of and or but with for from by as if when because which \
who my your our their his its is are was were be been will would can could should have has had \
into about than very really that this these those we you they it he she there then so not just \
also what how in on at up out off over under again more most some any all each every other \
another such only own same both few many much no nor too now here where why while after before \
during until since through between against without within along across behind near around \
go goes going went gone get gets getting got make makes making made do does doing did done say \
says said see sees seeing saw seen know knows knew think thinks thought take takes taking took \
come comes coming came want wants wanted look looks looking use uses using used find finds \
found give gives giving gave tell tells told work works working call calls called try tries \
trying tried ask asks asked need needs needed feel feels felt become leave leaves left put \
keep keeps kept let begin seem help helps talk turn start starts started show shows showed \
hear play run move like live believe hold bring happen write provide sit stand lose pay meet \
include continue set learn change lead understand watch follow stop create speak read allow \
add spend grow open walk win offer remember consider appear buy wait serve send sends sending \
sent expect build stay fall cut reach remain suggest raise pass sell require report decide \
pull check checks checking update updates share order orders ship ships shipping print book \
time year people way day man thing woman life child world school state family student group \
country problem hand part place case week company system program question number night point \
home water room mother area money story fact month lot right study business issue side kind \
head house service friend power hour game line end member law car city community name team \
minute idea body information back parent face others level office door health person art \
war history party result morning reason research moment air teacher force education price \
product products customer customers client clients invoice invoices offer offers email emails \
message messages meeting call list file files page pages link version text word words sentence \
good new first last long great little old big high different small large next early young \
important public bad able free sure clear ready short simple quick better best nice please \
thanks thank yes okay well maybe actually basically already always never often still even \
together today tomorrow yesterday tonight one two three four five six seven eight nine ten";

fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn lower_first(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_lowercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// Joins two neighbouring chunks' texts. Each was decoded alone, so each
/// starts with a capital and ends with a full stop, even where the speaker
/// only took a breath. Only the clear cases are mended; the rest stays as the
/// speech model wrote it.
fn mend(before: &mut String, after: &mut String) {
    let Some(last) = before.split_whitespace().last().map(str::to_string) else {
        return;
    };
    let Some(first) = after.split_whitespace().next().map(str::to_string) else {
        return;
    };
    let ended = before.trim_end().ends_with('.') && !before.trim_end().ends_with("..");
    if !ended {
        return;
    }
    let first_word = bare(&first);
    let carries_on = CARRIES_ON.contains(&first_word.as_str());
    let cut_short = NEVER_LAST.contains(&bare(&last).as_str());
    if !carries_on && !cut_short {
        return;
    }
    let trimmed = before.trim_end().trim_end_matches('.').to_string();
    *before = if carries_on && !cut_short {
        format!("{trimmed},")
    } else {
        trimmed
    };
    if PLAIN_WORDS
        .split_whitespace()
        .any(|plain| plain == first_word)
    {
        *after = lower_first(after.trim_start());
    }
}

/// The chunks' texts as one text. Called in place of upstream's join, which
/// still does the joining once the seams are mended.
pub fn join<S: AsRef<str>>(texts: &[S]) -> String {
    let mut parts: Vec<String> = texts
        .iter()
        .map(|text| text.as_ref().trim().to_string())
        .filter(|text| !text.is_empty())
        .collect();
    for index in 1..parts.len() {
        let (head, tail) = parts.split_at_mut(index);
        mend(&mut head[index - 1], &mut tail[0]);
    }
    join_transcripts(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::vad::{VadFrame, VoiceActivityDetector};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const FRAME: usize = 512;

    /// Scores a frame by its first sample, as upstream's own tests do.
    struct Scripted {
        fail: bool,
    }

    impl VoiceActivityDetector for Scripted {
        fn push_frame<'a>(&'a mut self, _frame: &'a [f32]) -> anyhow::Result<VadFrame<'a>> {
            unreachable!("chunking only scores")
        }

        fn frame_samples(&self) -> usize {
            FRAME
        }
    }

    impl SpeechScorer for Scripted {
        fn score(&mut self, frames: &[f32]) -> anyhow::Result<Vec<f32>> {
            assert!(frames.len() % FRAME == 0, "whole frames only");
            if self.fail {
                anyhow::bail!("scripted failure");
            }
            Ok(frames.chunks(FRAME).map(|frame| frame[0]).collect())
        }
    }

    fn chunker_with(fail: bool) -> (Chunker, Arc<AtomicUsize>) {
        let made = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&made);
        let chunker = Chunker::new(Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(Scripted { fail }) as Box<dyn SpeechScorer>)
        }));
        (chunker, made)
    }

    fn chunker() -> Chunker {
        chunker_with(false).0
    }

    fn frames(seconds: f64) -> usize {
        (seconds * RATE as f64 / FRAME as f64).round() as usize
    }

    /// Audio made of stretches of speech and silence, in seconds. Every
    /// sample carries its own index after the first of each frame, so a lost
    /// or doubled sample shows.
    fn audio(stretches: &[(f64, bool)]) -> Vec<f32> {
        let mut pcm = Vec::new();
        for &(seconds, speech) in stretches {
            for _ in 0..frames(seconds) {
                pcm.push(if speech { 0.9 } else { 0.05 });
                let at = pcm.len();
                pcm.extend((1..FRAME).map(|i| (at + i) as f32));
            }
        }
        pcm
    }

    /// Pushes `pcm` the way the recorder does, in small uneven pieces.
    /// Returns the chunks that arrived while recording, and those at the end.
    fn live(chunker: &mut Chunker, pcm: &[f32]) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let mut early = Vec::new();
        for piece in pcm.chunks(FRAME + 37) {
            early.extend(chunker.push(piece.to_vec()));
        }
        (early, chunker.finish())
    }

    fn seconds(chunks: &[Vec<f32>]) -> Vec<f64> {
        chunks
            .iter()
            .map(|chunk| (chunk.len() as f64 / RATE as f64 * 100.0).round() / 100.0)
            .collect()
    }

    #[test]
    fn a_short_dictation_is_one_chunk_and_loads_no_detector() {
        let pcm = audio(&[(2.0, true), (0.4, false)]);
        let (mut chunker, made) = chunker_with(false);
        let (early, late) = live(&mut chunker, &pcm);
        assert!(early.is_empty());
        assert_eq!(late, vec![pcm.clone()]);
        assert_eq!(made.load(Ordering::Relaxed), 0);
        assert_eq!(chunker.samples(), pcm.len());
    }

    #[test]
    fn a_pause_hands_over_the_words_before_it_while_still_recording() {
        let pcm = audio(&[(4.0, true), (0.45, false), (3.0, true)]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        // Cut once 350 ms of the pause have gone by: 11 frames of 32 ms.
        assert_eq!(seconds(&early), vec![4.35]);
        assert_eq!(late.len(), 1);
        let all: Vec<f32> = early.into_iter().chain(late).flatten().collect();
        assert_eq!(all, pcm, "every sample comes out once, in order");
    }

    #[test]
    fn several_pauses_leave_only_the_last_words_for_the_end() {
        let pcm = audio(&[
            (5.0, true),
            (0.45, false),
            (6.0, true),
            (0.45, false),
            (4.0, true),
            (0.45, false),
            (2.0, true),
        ]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        assert_eq!(early.len(), 3);
        // The 100 ms of pause left over, then the last two seconds.
        assert_eq!(seconds(&late), vec![2.11]);
    }

    #[test]
    fn the_silence_after_the_last_word_is_not_decoded() {
        let pcm = audio(&[(4.0, true), (0.45, false)]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        assert_eq!(seconds(&early), vec![4.35]);
        assert!(late.is_empty(), "the tail is silence");
        // The decoder checks this against the recording before it trusts us.
        assert_eq!(chunker.samples(), pcm.len());
    }

    #[test]
    fn silence_is_only_dropped_after_a_cut() {
        // Never the whole recording, however quiet it scores.
        let pcm = audio(&[(0.9, false)]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        assert!(early.is_empty());
        assert_eq!(late, vec![pcm]);
    }

    #[test]
    fn a_breath_is_not_a_pause() {
        let pcm = audio(&[(4.0, true), (0.2, false), (4.0, true)]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        assert!(early.is_empty());
        assert_eq!(late, vec![pcm]);
    }

    #[test]
    fn a_pause_too_early_waits_for_enough_speech() {
        // One word, a long pause, then the sentence: nothing to cut yet.
        let pcm = audio(&[(0.6, true), (0.45, false), (5.0, true), (0.45, false)]);
        let mut chunker = chunker();
        let (early, _) = live(&mut chunker, &pcm);
        assert_eq!(seconds(&early), vec![6.4]);
    }

    #[test]
    fn long_speech_without_a_pause_is_cut_at_its_best_breath() {
        let pcm = audio(&[
            (5.0, true),
            (0.13, false),
            (4.0, true),
            (0.26, false),
            (6.0, true),
        ]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        // The longer breath, in its middle: 9.12 s of audio and half of 0.26.
        assert_eq!(seconds(&early), vec![9.25]);
        let all: Vec<f32> = early.into_iter().chain(late).flatten().collect();
        assert_eq!(all, pcm);
    }

    #[test]
    fn speech_with_no_gap_at_all_is_cut_hard_at_thirty_seconds() {
        let pcm = audio(&[(31.0, true)]);
        let mut chunker = chunker();
        let (early, late) = live(&mut chunker, &pcm);
        assert_eq!(early.len(), 1);
        let cut = seconds(&early)[0];
        assert!((30.0..30.1).contains(&cut), "cut at {cut}");
        assert_eq!(late.len(), 1);
    }

    #[test]
    fn a_detector_that_fails_leaves_the_recording_in_one_piece() {
        let pcm = audio(&[(4.0, true), (0.45, false), (3.0, true)]);
        let (mut chunker, _) = chunker_with(true);
        let (early, late) = live(&mut chunker, &pcm);
        assert!(early.is_empty());
        assert_eq!(late, vec![pcm]);
    }

    #[test]
    fn a_whole_clip_goes_through_upstreams_chunker() {
        let pcm = audio(&[(4.0, true), (0.45, false), (3.0, true)]);
        let mut chunker = chunker();
        chunker.whole_clip();
        let mut chunks = chunker.push(pcm.clone());
        chunks.extend(chunker.finish());
        // Upstream never cuts a clip of 40 seconds or less.
        assert_eq!(chunks, vec![pcm.clone()]);
        assert_eq!(chunker.samples(), pcm.len());
    }

    #[test]
    fn seams_inside_a_sentence_are_mended() {
        assert_eq!(
            join(&["I want to.", "Go to the market tomorrow."]),
            "I want to go to the market tomorrow."
        );
        assert_eq!(
            join(&["We ship on Friday.", "And then we send the invoice."]),
            "We ship on Friday, and then we send the invoice."
        );
        assert_eq!(
            join(&["Please send it to.", "Marlow before lunch."]),
            "Please send it to Marlow before lunch."
        );
    }

    #[test]
    fn real_sentence_ends_are_left_alone() {
        assert_eq!(
            join(&["We ship on Friday.", "The invoice follows on Monday."]),
            "We ship on Friday. The invoice follows on Monday."
        );
        assert_eq!(
            join(&["Is it ready?", "And who checks it?"]),
            "Is it ready? And who checks it?"
        );
        assert_eq!(join(&["One chunk only."]), "One chunk only.");
        assert_eq!(join(&["First.", "", "  ", "Third."]), "First. Third.");
        assert_eq!(join::<&str>(&[]), "");
    }
}
