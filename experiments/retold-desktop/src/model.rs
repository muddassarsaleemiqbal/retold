//! Session-only creative drafts. Nothing here grants production or export authority.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    English,
    Hindi,
    Urdu,
}

impl Edition {
    pub const ALL: [Self; 3] = [Self::English, Self::Hindi, Self::Urdu];

    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Hindi => "हिन्दी",
            Self::Urdu => "اردو",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::English => 0,
            Self::Hindi => 1,
            Self::Urdu => 2,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub narration: String,
    pub note: String,
    pub approved: bool,
}

#[derive(Debug)]
pub struct Scene {
    pub title: &'static str,
    pub duration: &'static str,
    drafts: [Draft; 3],
}

#[derive(Debug)]
pub struct ReviewSession {
    scenes: Vec<Scene>,
    pub selected: usize,
    pub edition: Edition,
}

impl Default for ReviewSession {
    fn default() -> Self {
        Self {
            scenes: fixtures(),
            selected: 1,
            edition: Edition::English,
        }
    }
}

impl ReviewSession {
    pub fn scenes(&self) -> &[Scene] {
        &self.scenes
    }

    pub fn scene(&self) -> &Scene {
        &self.scenes[self.selected]
    }

    pub fn draft(&self) -> &Draft {
        &self.scene().drafts[self.edition.index()]
    }

    pub fn save(&mut self, narration: String, note: String) {
        let draft = &mut self.scenes[self.selected].drafts[self.edition.index()];
        if draft.narration != narration {
            draft.approved = false;
        }
        draft.narration = narration;
        draft.note = note;
    }

    pub fn approve(&mut self) {
        self.scenes[self.selected].drafts[self.edition.index()].approved = true;
    }

    pub fn select(&mut self, index: usize) {
        if index < self.scenes.len() {
            self.selected = index;
        }
    }

    pub fn remaining(&self) -> usize {
        self.scenes
            .iter()
            .filter(|scene| !scene.drafts[self.edition.index()].approved)
            .count()
    }
}

fn fixtures() -> Vec<Scene> {
    [
        (
            "Harbor morning",
            "00:42",
            [
                "At first light, the harbor was already awake. The keeper watched the boats leave, just as he had for thirty years.",
                "सुबह की पहली रोशनी में बंदरगाह जाग चुका था। रखवाला तीस वर्षों की तरह आज भी नावों को जाते देख रहा था।",
                "صبح کی پہلی روشنی میں بندرگاہ جاگ چکی تھی۔ نگہبان تیس برس کی طرح آج بھی کشتیوں کو جاتے دیکھ رہا تھا۔",
            ],
        ),
        (
            "The last evening",
            "00:56",
            [
                "By evening, the boats had returned to shore. Only the lighthouse keeper noticed that one light was still moving across the water.",
                "शाम तक नावें किनारे लौट आई थीं। केवल प्रकाशस्तंभ के रखवाले ने देखा कि एक रोशनी अब भी पानी पर आगे बढ़ रही थी।",
                "شام تک کشتیاں ساحل پر لوٹ آئی تھیں۔ صرف مینار کے نگہبان نے دیکھا کہ ایک روشنی اب بھی پانی پر آگے بڑھ رہی تھی۔",
            ],
        ),
        (
            "A distant signal",
            "00:38",
            [
                "Three short flashes broke the quiet. He reached for the radio, then stopped. Someone on the headland was answering.",
                "तीन छोटी चमक ने सन्नाटा तोड़ दिया। उसने रेडियो की ओर हाथ बढ़ाया, फिर रुक गया। कोई किनारे से जवाब दे रहा था।",
                "تین مختصر چمکوں نے خاموشی توڑ دی۔ اس نے ریڈیو کی طرف ہاتھ بڑھایا، پھر رک گیا۔ کوئی ساحل سے جواب دے رہا تھا۔",
            ],
        ),
    ]
    .into_iter()
    .map(|(title, duration, narrations)| Scene {
        title,
        duration,
        drafts: narrations.map(|narration| Draft {
            narration: narration.into(),
            ..Draft::default()
        }),
    })
    .collect()
}
