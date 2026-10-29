// Deterministic providers for full-workflow contract tests. They make no network/editor calls.
use std::collections::BTreeMap;
use std::sync::{Arc,Mutex,atomic::{AtomicUsize,AtomicBool,Ordering}};
use async_trait::async_trait;
use retold_core::{LanguageTag,Tier,Usd};
use retold_engine::{NewRun,Quality,Stage,fsx,run};
use retold_engine::assemble::FontResolver;
use retold_engine::context::Context;
use retold_engine::ledger::Ledger;
use retold_engine::media_runtime::{MediaError,MediaRegistry,MediaRuntime};
use retold_engine::store::Store;
use retold_guide::channel::init_from_template;
use retold_providers::{Billing,ProviderError,Router,TextProvider,TextRequest,TextResponse,Usage,ProvidersConfig};
use retold_providers::media::{ImageRequest,Media,MediaProvider,MediaUsage,Quote,SpeechRequest,Transcript,Word};
use serde_json::{Value,json};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug,Default)]
pub struct TextFake{pub per_use:bool,pub calls:AtomicUsize,pub qa:AtomicUsize,pub reject_first_frame:bool,pub reject_first_idiom:AtomicBool}

#[async_trait]
impl TextProvider for TextFake{
 fn id(&self)->&'static str{"fixture"} fn billing(&self)->Billing{if self.per_use{Billing::PerUse}else{Billing::Subscription}} fn supports_images(&self)->bool{true} fn resolve_model(&self,s:&str)->String{s.into()}
 async fn generate(&self,req:&TextRequest,_:&str,_:&CancellationToken)->Result<TextResponse,ProviderError>{
  self.calls.fetch_add(1,Ordering::SeqCst);
  let prompt=&req.prompt;
  let schema=&req.schema;
  let p=&schema["properties"];
  let wrap = |value| Ok(TextResponse{value,usage:Usage::default()});
  if prompt.contains("Respell the following") {
      return wrap(json!({"text":prompt.split("Expected: ").nth(1).unwrap().split(". Heard:").next().unwrap()}));
  }
  if p.get("beats").is_some() {
      return wrap(serde_json::from_str(include_str!("../fixtures/full/plan.json")).unwrap());
  }
  if p.get("candidates").is_some() {
      return wrap(serde_json::from_str(include_str!("../fixtures/full/hooks.json")).unwrap());
  }
  if p.get("winner").is_some() {
      let count=if prompt.contains("Blind drafts"){2}else{5};
      return wrap(json!({"winner":0,"scores":(0..count).map(|_|9.0).collect::<Vec<_>>(),"reason":"fixture selection"}));
  }
  if p.get("sections").is_some() {
      return wrap(serde_json::from_str(include_str!("../fixtures/full/script.json")).unwrap());
  }
  if p.get("faces").is_some() {
      return wrap(json!({"faces":[]}));
  }
  if p.get("issues").is_some() && prompt.contains("idioms and cultural references") && self.reject_first_idiom.swap(false,Ordering::SeqCst) {
      return wrap(json!({"issues":[{"claim":"recorded literal idiom","problem":"idiom loses the intended meaning in Hindi","evidence":""}]}));
  }
  if p.get("issues").is_some() {
      return wrap(json!({"issues":[]}));
  }
  if p.get("shots").is_some() {
      let units=after(prompt,"Units: ");
      return wrap(json!({"shots":units.as_array().unwrap().iter().enumerate().map(|(unit_idx,unit_item)|json!({"unit":unit_item["id"],"shot":if unit_idx%2==0{"wide"}else{"medium"},"location":"Wisborg","action":"An original character waits beside a window","mood":"grave","characters":[],"focus":[0.5,0.4],"motion":if unit_idx<2&&prompt.contains("Parallax rules: enabled=true"){Some("parallax")}else{None},"parallax_move":if unit_idx<2&&prompt.contains("Parallax rules: enabled=true"){Some("orbit")}else{None},"hero":unit_idx==0})).collect::<Vec<_>>()}));
  }
  if p.get("characters").is_some() {
      let names=after(prompt,"names exactly once: ");
      return wrap(json!({"characters":names.as_array().unwrap().iter().map(|character_name|json!({"name":character_name,"appearance":"Oval face and expressive brows","clothes":"Plain slate coat","palette":[format!("#{:06x}",character_name.as_str().unwrap().len()*0x01_0203 % 0x00ff_ffff),"#EEE6D7".to_string()],"distinguishing_detail":"A narrow cream scarf","age":"late thirties","build":"7.5 heads tall, lean, upright","face":"Oval head, dot eyes, thin brows, level mouth","hair":"Short dark side-parted hair","voice":"male, late thirties, warm","colors":[{"part":"skin","hex":"#E8C9A8"},{"part":"coat","hex":format!("#{:06x}",character_name.as_str().unwrap().len()*0x01_0203 % 0x00ff_ffff)},{"part":"scarf","hex":"#EEE6D7"}]})).collect::<Vec<_>>()}));
  }
  if p.get("same_character").is_some() {
      let figures=prompt.split("exactly ").nth(1).and_then(|t|t.split(' ').next()?.parse::<u32>().ok()).unwrap_or(1);
      return wrap(json!({"figures":figures,"same_character":true,"text_present":false,"anatomy_errors":[],"problems":[]}));
  }
  if p.get("style_score").is_some() {
      let first=self.qa.fetch_add(1,Ordering::SeqCst)==0&&self.reject_first_frame;
      return wrap(json!({"style_score":if first{5.0}else{9.0},"character_score":9.0,"text_present":first,"anatomy_errors":[],"problems":if first{vec!["remove lettering"]}else{vec![]},"focus":[0.5,0.4]}));
  }
  if p.get("units").is_some() {
      let master=after(prompt,"Master: ");let ur=prompt.contains("Urdu");let count=master["units"].as_array().unwrap().len();let base=280/count;let remainder=280%count;
      return wrap(json!({"units":master["units"].as_array().unwrap().iter().enumerate().map(|(unit_idx,unit_item)|{
          let n=base+usize::from(unit_idx<remainder);let mut words:Vec<String>=(0..n).map(|word_idx|if ur{format!("لفظ{}",unit_idx*n+word_idx)}else{format!("शब्द{}",unit_idx*n+word_idx)}).collect();
          let stop=if ur{'۔'}else{'।'};let ask=if ur{'؟'}else{'?'};if unit_idx%3==0{words[n/2-1].push(stop);}words[n-1].push(if unit_idx+1==count{ask}else{stop});
          json!({"id":unit_item["id"],"section":unit_item["section"],"text":words.join(" ")})
      }).collect::<Vec<_>>()}));
  }
  if p.get("words").is_some() {
      let input=after(prompt,"Input: ");
      return wrap(json!({"words":input.as_array().unwrap().iter().enumerate().map(|(word_idx,_)|format!("शब्द{word_idx}")).collect::<Vec<_>>()}));
  }
  if p.get("phrases").is_some() {
      let input=after(prompt,"Phrases: ");
      return wrap(json!({"phrases":input.as_array().unwrap().iter().map(|phrase_entry|json!({"id":phrase_entry["id"],"text":"The visitor waits at the window."})).collect::<Vec<_>>()}));
  }
  if p.get("posts").is_some() {
      return wrap(json!({"posts":(["youtube","tiktok","instagram","facebook"].into_iter().map(|platform_name|json!({"platform":platform_name,"title":"Nosferatu: the open window","description":"Spoilers for Nosferatu. An animated recap with AI-generated art.","hashtags":["#Nosferatu","#Story","#Recap","#Animation","#Film"]})).collect::<Vec<_>>())}));
  }
  Err(ProviderError::Malformed(format!("unhandled fixture schema: {schema}")))
 }
}
fn after(prompt:&str,marker:&str)->Value{let tail=prompt.split_once(marker).unwrap().1;serde_json::Deserializer::from_str(tail).into_iter::<Value>().next().unwrap().unwrap()}

#[derive(Debug,Default)]
pub struct MediaFake{pub texts_file:Option<std::path::PathBuf>,pub id_override:Option<&'static str>,pub unavailable_speech:AtomicBool,pub small_images:AtomicBool,pub mouth_sheets:AtomicBool,pub recorded_speech:AtomicBool,pub inaccurate_speech:AtomicBool,pub images:AtomicUsize,pub image_prompts:Mutex<Vec<String>>,pub speech:AtomicUsize,pub transcripts:AtomicUsize,pub active:AtomicUsize,pub max_active:AtomicUsize,pub texts:Mutex<BTreeMap<String,String>>,pub paid:bool,pub block_second_quote:AtomicBool,pub quote_blocked:AtomicBool,pub fail_speech:AtomicBool,pub invalid_transcript:AtomicBool}
impl MediaFake {
 fn record_image_prompt(&self,req:&ImageRequest){let mut prompts=self.image_prompts.lock().unwrap();prompts.push(req.prompt.clone());}
 const fn quote(&self)->Quote{Quote{ceiling:if self.paid{Usd::from_micros(10_000)}else{Usd::ZERO},provider_tag:None}}
 fn usage(&self)->MediaUsage{MediaUsage{generation:Some("fixture".into()),cost:Some(if self.paid{Usd::from_micros(5_000)}else{Usd::ZERO})}}
}
#[async_trait]
impl MediaProvider for MediaFake{
 fn id(&self)->&'static str{self.id_override.unwrap_or("openrouter")}fn billing(&self)->Billing{if self.paid{Billing::PerUse}else{Billing::Subscription}}
 async fn text_quote(&self,_:&str,_:&TextRequest,_:&CancellationToken)->Result<Quote,ProviderError>{Ok(self.quote())}
 async fn image_quote(&self,_:&str,_:&ImageRequest,_:&CancellationToken)->Result<Quote,ProviderError>{Ok(self.quote())}
 async fn speech_quote(&self,_:&str,_:&SpeechRequest,cancel:&CancellationToken)->Result<Quote,ProviderError>{if self.block_second_quote.load(Ordering::SeqCst)&&self.speech.load(Ordering::SeqCst)==1{self.quote_blocked.store(true,Ordering::SeqCst);cancel.cancelled().await;return Err(ProviderError::Cancelled);}Ok(self.quote())}
 async fn transcript_quote(&self,_:&str,_:f64,_:&CancellationToken)->Result<Quote,ProviderError>{Ok(self.quote())}
 async fn image(&self,_:&str,req:&ImageRequest,_:&Quote,cancel:&CancellationToken)->Result<Media<Vec<u8>>,ProviderError>{self.images.fetch_add(1,Ordering::SeqCst);self.record_image_prompt(req);let active=self.active.fetch_add(1,Ordering::SeqCst)+1;self.max_active.fetch_max(active,Ordering::SeqCst);tokio::select!{()=cancel.cancelled()=>return Err(ProviderError::Cancelled),()=tokio::time::sleep(std::time::Duration::from_millis(8))=>{}}self.active.fetch_sub(1,Ordering::SeqCst);Ok(Media{value:if self.mouth_sheets.load(Ordering::SeqCst)&&(req.prompt.contains("mouth and eye sheet")||req.prompt.contains("Generate ONE square")){mouth_png(req)}else if self.small_images.load(Ordering::SeqCst){small_png()}else{png()},usage:self.usage()})}
 async fn speech(&self,_:&str,req:&SpeechRequest,_:&CancellationToken)->Result<Media<Vec<u8>>,ProviderError>{self.speech.fetch_add(1,Ordering::SeqCst);if self.unavailable_speech.load(Ordering::SeqCst){return Err(ProviderError::Unavailable("missing local model".into()));}
if self.fail_speech.load(Ordering::SeqCst){return Err(ProviderError::Timeout("ambiguous fixture timeout".into()));}let re=regex::Regex::new(r"\[[^\]]*\]").unwrap();{let mut texts=self.texts.lock().unwrap();texts.insert(req.language.to_string(),re.replace_all(&req.text,"").into_owned());if let Some(path)=&self.texts_file{fsx::write_json_atomic(path,&*texts).unwrap();}}Ok(Media{value:if self.recorded_speech.load(Ordering::SeqCst){include_bytes!("../../../retold-voice/tests/fixtures/speech.wav").to_vec()}else{wav(100)},usage:self.usage()})}
 async fn transcribe(&self,_:&str,_:&[u8],_:&str,lang:&LanguageTag,_:&CancellationToken)->Result<Media<Transcript>,ProviderError>{self.transcripts.fetch_add(1,Ordering::SeqCst);let text=self.texts.lock().unwrap().get(&lang.to_string()).unwrap().clone();let tokens:Vec<_>=text.split_whitespace().collect();let duration=if self.recorded_speech.load(Ordering::SeqCst){4.76}else{100.0};let step=duration/f64::from(u32::try_from(tokens.len()).unwrap());let mut words:Vec<Word>=tokens.iter().enumerate().map(|(i,w)|Word{word:(*w).into(),start:f64::from(u32::try_from(i).unwrap())*step,end:f64::from(u32::try_from(i+1).unwrap())*step-0.002}).collect();if self.invalid_transcript.load(Ordering::SeqCst){words[0].end=words[0].start;}Ok(Media{value:Transcript{text:if self.inaccurate_speech.load(Ordering::SeqCst){"wrong unrelated words".into()}else{text},duration,words},usage:self.usage()})}
}
fn png()->Vec<u8>{static PNG:std::sync::OnceLock<Vec<u8>>=std::sync::OnceLock::new();PNG.get_or_init(||{let image=image::RgbImage::from_pixel(720,1280,image::Rgb([230,220,205]));let mut data=std::io::Cursor::new(Vec::new());image::DynamicImage::ImageRgb8(image).write_to(&mut data,image::ImageFormat::Png).unwrap();data.into_inner()}).clone()}
fn wav(seconds:u32)->Vec<u8>{let samples=16_000*seconds;let bytes=samples*2;let mut out=Vec::new();out.extend(b"RIFF");out.extend((36+bytes).to_le_bytes());out.extend(b"WAVEfmt ");out.extend(16u32.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(16_000u32.to_le_bytes());out.extend(32_000u32.to_le_bytes());out.extend(2u16.to_le_bytes());out.extend(16u16.to_le_bytes());out.extend(b"data");out.extend(bytes.to_le_bytes());out.resize(44+bytes as usize,0);out}

#[derive(Debug)]pub struct EstimatedFonts;
impl FontResolver for EstimatedFonts{fn face(&self,_:&retold_guide::channel::FontSpec)->Result<Box<dyn retold_lang::fit::Measure>,MediaError>{Ok(Box::new(retold_lang::fit::Estimate))}}

pub fn setup(variants:&[&str],budget:Usd)->(tempfile::TempDir,Arc<Context>,Arc<TextFake>,Arc<MediaFake>){
 let paid=budget>Usd::ZERO;
 let tmp=tempfile::tempdir().unwrap();let guide=retold_guide::catalog::bundled().unwrap().remove(0);let channel=init_from_template("test-studio",&tmp.path().join("config/channels/test-studio")).unwrap();
 let raw:indexmap::IndexMap<_,_>=[("title".into(),"Nosferatu".into()),("kind".into(),"film".into()),("year".into(),"1922".into())].into();
 let(layout,meta)=run::create(&tmp.path().join("library"),NewRun{guide:&guide,channel:&channel,inputs:retold_guide::inputs::resolve(guide.manifest(),&raw).unwrap(),variants:variants.iter().map(|v|v.parse().unwrap()).collect(),quality:Quality::Standard,budget,skip_reviews:true,source_file:None}).unwrap();
 fsx::write_atomic(&layout.stage(Stage::Research).join("source-file.txt"),include_bytes!("../fixtures/full/source.txt")).unwrap();
 let text=Arc::new(TextFake{reject_first_frame:true,..TextFake::default()});let media=Arc::new(MediaFake{paid,..MediaFake::default()});
 let mut config=ProvidersConfig::suggested(retold_providers::config::Found{claude:false,codex:false});
 for tier in Tier::ALL{config.routing.insert(tier,vec![if matches!(tier,Tier::Image|Tier::ImageNoRefs|Tier::Tts|Tier::Transcribe){"openrouter:model"}else{"fixture:model"}.parse().unwrap()]);}
 let registry=Arc::new(MediaRegistry::from_providers(config,vec![(Arc::clone(&media) as Arc<dyn MediaProvider>,2)]));
 let router=Arc::new(Router::from_providers(vec![(Arc::clone(&text) as Arc<dyn TextProvider>,2)],registry.config.routing.clone()).with_retry_backoff(std::time::Duration::ZERO));
 let store=Arc::new(AsyncMutex::new(Store::open(&layout).unwrap()));let ledger=Arc::new(Ledger::open(&layout.meta().join("ledger.json"),budget).unwrap());
 let context=Arc::new(Context{layout,meta,guide,channel,text:router,media:MediaRuntime{registry,store,ledger,cancel:CancellationToken::new(),allow_paid:paid},skip_reviews:true,depth:Arc::new(retold_parallax::depth::ModelDepth::default()),phonemes:Arc::new(retold_engine::phonemes::LocalAligner)});
 (tmp,context,text,media)
}

fn small_png()->Vec<u8>{include_bytes!("../../../retold-parallax/tests/fixtures/scene-0.png").to_vec()}

fn mouth_png(req:&ImageRequest)->Vec<u8>{let image=retold_lipsync::sheet::synthetic(96);let image=if req.prompt.contains("Generate ONE square"){image::imageops::crop_imm(&image,192,96,96,96).to_image()}else{image};let mut data=std::io::Cursor::new(Vec::new());image::DynamicImage::ImageRgba8(image).write_to(&mut data,image::ImageFormat::Png).unwrap();data.into_inner()}
