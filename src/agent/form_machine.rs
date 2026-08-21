use crate::models::{FormStep, UserFormState, WorkerProfile};
use chrono::Utc;
use dashmap::DashMap;
use std::sync::Arc;
use tracing::info;

pub struct FormMachine {
    states: Arc<DashMap<String, UserFormState>>,
}

impl FormMachine {
    pub fn new() -> Self {
        Self {
            states: Arc::new(DashMap::new()),
        }
    }

    pub fn get_state(&self, phone: &str) -> UserFormState {
        self.states
            .get(phone)
            .map(|r| r.value().clone())
            .unwrap_or_else(|| UserFormState {
                step: FormStep::NotStarted,
                profile: WorkerProfile {
                    phone_number: phone.to_string(),
                    ..Default::default()
                },
            })
    }

    pub fn set_state(&self, phone: &str, state: UserFormState) {
        self.states.insert(phone.to_string(), state);
    }

    pub fn start_form(&self, phone: &str) -> String {
        let new_state = UserFormState {
            step: FormStep::AskFullName,
            profile: WorkerProfile {
                phone_number: phone.to_string(),
                registered_at: Utc::now().to_rfc3339(),
                ..Default::default()
            },
        };
        self.set_state(phone, new_state);

        "أهلاً بك في نظام التسجيل المعتمد لدى ماعت.\nيرجى كتابة اسمك الثلاثي بالكامل:".to_string()
    }

    pub fn process_step(&self, phone: &str, input_text: &str) -> (String, Option<WorkerProfile>) {
        let mut state = self.get_state(phone);
        let cleaned_input = input_text.trim();

        match state.step {
            FormStep::NotStarted => {
                let msg = self.start_form(phone);
                (msg, None)
            }
            FormStep::AskFullName => {
                state.profile.full_name = cleaned_input.to_string();
                state.step = FormStep::AskJobRole;
                self.set_state(phone, state);
                ("تمام تسلم. ما هي وظيفتك أو صنعتك بالضبط؟ (مثال: نجار، حداد، مهندس، سائق...)".to_string(), None)
            }
            FormStep::AskJobRole => {
                state.profile.job_role = cleaned_input.to_string();
                state.step = FormStep::AskNationalId;
                self.set_state(phone, state);
                ("تمام. يرجى إرسال رقم البطاقة الشخصية (الرقم القومي) أو رقم التليفون للتحقق:".to_string(), None)
            }
            FormStep::AskNationalId => {
                state.profile.national_id = cleaned_input.to_string();
                state.step = FormStep::AskDailyRate;
                self.set_state(phone, state);
                ("الله ينور. كم يوميتك أو أجرك اليومي بالجنيه؟".to_string(), None)
            }
            FormStep::AskDailyRate => {
                state.profile.daily_rate = cleaned_input.to_string();
                state.step = FormStep::AskWorkLocation;
                self.set_state(phone, state);
                ("ما اسم موقع العمل أو الموقع اللي شغال فيه حالياً؟".to_string(), None)
            }
            FormStep::AskWorkLocation => {
                state.profile.work_location = cleaned_input.to_string();
                state.step = FormStep::AskNotes;
                self.set_state(phone, state);
                ("هل عندك أي ملاحظات إضافية أو معدات استلمتها؟ (اكتب 'لا يوجد' لو مفيش)".to_string(), None)
            }
            FormStep::AskNotes => {
                state.profile.notes = cleaned_input.to_string();
                state.step = FormStep::Completed;
                let completed_profile = state.profile.clone();
                self.set_state(phone, state);

                info!("Completed worker application for {}", phone);
                let response = format!(
                    "تم تسجيل بياناتك بنجاح في قاعدة بيانات ماعت!\n\nالاسم: {}\nالوظيفة: {}\nالموقع: {}\nاليومية: {}\n\nشكراً لك، وسنقوم بمتابعة يومياتك وشغلك بانتظام.",
                    completed_profile.full_name,
                    completed_profile.job_role,
                    completed_profile.work_location,
                    completed_profile.daily_rate
                );
                (response, Some(completed_profile))
            }
            FormStep::Completed => {
                ("بياناتك مسجلة لدينا بالفعل. يمكنك إرسال أي تقرير يومي أو استفسار وسأقوم بتسجيله فوراً.".to_string(), None)
            }
        }
    }
}
