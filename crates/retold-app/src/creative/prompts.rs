//! Cinematographic prompt expansion and negative prompt compiler.

use crate::creative::types::{
    CameraOptics, CinematicLighting, ColorGrading, EnhancedPrompt, ModelTarget,
    PromptEnhanceOptions,
};

const WORDS_TO_TOKENS_NUMERATOR: usize = 4;
const WORDS_TO_TOKENS_DENOMINATOR: usize = 3;

const fn describe_extended_lighting(lighting: CinematicLighting) -> &'static str {
    match lighting {
        CinematicLighting::Chiaroscuro => {
            "high-contrast chiaroscuro with pronounced deep shadows and stark highlights"
        }
        CinematicLighting::NeonNoir => {
            "cyberpunk neon-noir illumination with reflections of cyan and magenta practicals"
        }
        _ => "gentle softbox diffused daylight wrapping contours evenly",
    }
}

const fn describe_lighting(lighting: CinematicLighting) -> &'static str {
    match lighting {
        CinematicLighting::Rembrandt => {
            "Rembrandt lighting with soft triangular key and gentle shadow fill"
        }
        CinematicLighting::VolumetricGodRays => {
            "dramatic volumetric god rays piercing subtle atmospheric haze"
        }
        CinematicLighting::GoldenHour => {
            "warm low-angle golden hour sunlight with shimmering backlighting"
        }
        _ => describe_extended_lighting(lighting),
    }
}

const fn describe_extended_optics(optics: CameraOptics) -> &'static str {
    match optics {
        CameraOptics::Telephoto85mm => {
            "shot on 85mm portrait telephoto lens, creamy background compression, razor-sharp subject isolation"
        }
        _ => "shot on 100mm macro lens, ultra-detailed tactile texture, microscopic precision",
    }
}

const fn describe_optics(optics: CameraOptics) -> &'static str {
    match optics {
        CameraOptics::Anamorphic35mm => {
            "shot on 35mm anamorphic prime lens, cinematic oval bokeh, subtle horizontal lens flare"
        }
        CameraOptics::Prime50mm => {
            "shot on 50mm f/1.4 reference prime lens, natural human eye perspective, crisp optical focus"
        }
        CameraOptics::WideAngle24mm => {
            "shot on 24mm wide angle lens, expansive environmental perspective, dynamic depth"
        }
        _ => describe_extended_optics(optics),
    }
}

const fn describe_extended_color_grade(grade: ColorGrading) -> &'static str {
    match grade {
        ColorGrading::TealAndOrange => {
            "modern Hollywood teal and orange complementary grade, radiant warmth on subjects"
        }
        _ => "harsh bleach bypass film processing, intense edge contrast, desaturated silver sheen",
    }
}

const fn describe_color_grade(grade: ColorGrading) -> &'static str {
    match grade {
        ColorGrading::KodakPortra400 => {
            "Kodak Portra 400 filmic color tone, authentic subtle grain, organic skin tones"
        }
        ColorGrading::TechnicolorVibrant => {
            "rich 3-strip Technicolor look, deeply saturated primaries, timeless luster"
        }
        ColorGrading::MoodyDesaturated => {
            "moody muted desaturated palette, steel gray undertones, cinematic restraint"
        }
        _ => describe_extended_color_grade(grade),
    }
}

const fn model_negative_prompt(target: ModelTarget) -> &'static str {
    match target {
        ModelTarget::Flux => {
            "blurry, low quality, oversaturated, deformed anatomy, plastic skin texture, digital artifacts"
        }
        ModelTarget::Midjourney => {
            "deformed, distorted, extra limbs, bad eyes, poorly drawn hands, text, watermark, signature"
        }
        ModelTarget::Sdxl => {
            "ugly, deformed, disfigured, poor details, bad anatomy, duplicate, mutated hands, watermark"
        }
        ModelTarget::Recraft | ModelTarget::Generic => {
            "lowres, bad anatomy, bad hands, missing fingers, extra digit, cropped, worst quality, low quality"
        }
    }
}

fn estimate_token_count(text: &str) -> usize {
    let word_count = text.split_whitespace().count();
    (word_count * WORDS_TO_TOKENS_NUMERATOR) / WORDS_TO_TOKENS_DENOMINATOR
}

fn assemble_positive_prompt(
    base_prompt: &str,
    options: &PromptEnhanceOptions,
    descriptors: (&str, &str, &str),
) -> String {
    let (lighting_desc, optics_desc, grade_desc) = descriptors;
    match options.target {
        ModelTarget::Midjourney => {
            format!(
                "{base_prompt}, {lighting_desc}, {optics_desc}, {grade_desc} --ar 16:9 --style raw"
            )
        }
        _ => {
            format!(
                "{base_prompt}, {lighting_desc}, {optics_desc}, {grade_desc}, 8k UHD photographic masterpiece"
            )
        }
    }
}

#[must_use]
pub fn enhance_prompt(base_prompt: &str, options: &PromptEnhanceOptions) -> EnhancedPrompt {
    let lighting_desc = describe_lighting(options.lighting);
    let optics_desc = describe_optics(options.optics);
    let grade_desc = describe_color_grade(options.color_grade);

    let positive_prompt = assemble_positive_prompt(
        base_prompt,
        options,
        (lighting_desc, optics_desc, grade_desc),
    );
    let negative_prompt = model_negative_prompt(options.target).to_string();
    let estimated_tokens = estimate_token_count(&positive_prompt);
    let cinematic_enhancements = vec![
        lighting_desc.to_string(),
        optics_desc.to_string(),
        grade_desc.to_string(),
    ];

    EnhancedPrompt {
        target_model: options.target,
        positive_prompt,
        negative_prompt,
        estimated_tokens,
        cinematic_enhancements,
    }
}
