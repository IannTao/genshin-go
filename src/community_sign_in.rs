//! Community sign-in for the locally authorized account.
use crate::{
    launcher::{LauncherEnvironment, button},
    profile::{Profile, VerifiedAccount},
};
use nsh_core::{
    element::{computed, effect, timer},
    prelude::*,
};
use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc, time::Duration};

/// Represents one clock sample for community sign-in.
#[derive(Clone, Copy)]
pub struct Clock {
    /// Identifies the current Beijing calendar date as days since the Unix epoch.
    pub day: u64,
    /// Counts the time the system has spent suspended since boot.
    pub suspended: Duration,
}

/// Describes the day's completed community sign-in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    /// Indicates that the launcher completed sign-in.
    Signed,
    /// Indicates that sign-in was already complete.
    AlreadySigned,
}

/// Describes the confirmed sign-in and its reward status for the checked day.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignInResult {
    /// Indicates how sign-in completed.
    pub completion: Completion,
    /// Indicates whether the service confirmed that the award was received.
    pub reward_claimed: bool,
    /// Contains the claimed reward amount when the mission definition is available.
    pub reward_points: Option<u64>,
}

/// Describes a community sign-in failure and its recovery path.
#[derive(Clone, PartialEq, Eq)]
pub enum Failure {
    /// Indicates that the service request timed out.
    Timeout,
    /// Indicates that the saved authorization requires a new scan.
    Authorization,
    /// Indicates that the service requires manual verification.
    Verification,
    /// Contains a displayable error for other failures.
    Other(String),
}

/// Represents a community sign-in check at the application boundary.
pub type Operation = Pin<Box<dyn Future<Output = Result<SignInResult, Failure>>>>;

#[derive(Clone, PartialEq, Eq)]
enum Status {
    Pending,
    Complete(SignInResult),
    Failed(Failure),
}

impl Status {
    fn label(&self) -> String {
        match self {
            Self::Pending => "社区签到：待处理",
            Self::Complete(result) => match result.completion {
                Completion::Signed => "社区签到：成功",
                Completion::AlreadySigned => "社区签到：已签到",
            },
            Self::Failed(Failure::Timeout) => "社区签到：失败，网络超时，请重试",
            Self::Failed(Failure::Authorization) => "社区签到：失败，签到授权不可用，请重新扫码",
            Self::Failed(Failure::Verification) => "社区签到：失败，请在米游社完成验证或签到后重试",
            Self::Failed(Failure::Other(message)) => return format!("社区签到：失败，{message}"),
        }
        .into()
    }

    fn reward_label(&self) -> String {
        match self {
            Self::Pending => "待处理".into(),
            Self::Failed(_) => "失败".into(),
            Self::Complete(result) if result.reward_claimed => match result.reward_points {
                Some(points) => format!("已领取 · {points} 米游币"),
                None => "已领取 · 奖励数量暂不可用".into(),
            },
            Self::Complete(_) => "已签到 · 领取待确认".into(),
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Calendar {
    day: u64,
    entries: BTreeMap<u64, Status>,
    blocked: Option<Failure>,
}

impl Calendar {
    fn today(&self) -> &Status {
        self.entries.get(&self.day).unwrap_or(&Status::Pending)
    }

    fn status_label(&self) -> String {
        self.blocked.as_ref().map_or_else(
            || self.today().label(),
            |failure| Status::Failed(failure.clone()).label(),
        )
    }

    fn retry_label(&self) -> Option<&'static str> {
        if self.blocked == Some(Failure::Authorization) {
            return None;
        }
        match self.today() {
            Status::Complete(result)
                if !result.reward_claimed || result.reward_points.is_none() =>
            {
                Some("检查奖励领取")
            }
            Status::Failed(_) => Some("重试社区签到"),
            Status::Pending if self.blocked.is_some() => Some("重试社区签到"),
            Status::Complete(_) | Status::Pending => None,
        }
    }

    fn record(&mut self, status: Status) {
        self.blocked = match &status {
            Status::Failed(error @ (Failure::Authorization | Failure::Verification)) => {
                Some(error.clone())
            }
            _ => None,
        };
        if !matches!(status, Status::Failed(_)) || !matches!(self.today(), Status::Complete(_)) {
            self.entries.insert(self.day, status);
        }
    }

    fn advance(&mut self, day: u64) {
        self.day = day;
        self.entries
            .retain(|saved, _| (day.saturating_sub(6)..=day).contains(saved));
    }

    fn rows(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once((self.day, self.today()))
            .chain(
                self.entries
                    .range(..self.day)
                    .rev()
                    .map(|(&day, status)| (day, status)),
            )
            .filter_map(|(day, status)| {
                let date = chrono::NaiveDate::from_epoch_days(i32::try_from(day).ok()?)?;
                Some(format!("{date} · {}", status.reward_label()))
            })
    }
}

/// Shows the reward calendar for the saved account's community sign-in.
pub(crate) fn controls(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
) -> impl IntoElement {
    let account = computed(move || profile.get().account);
    let calendar = signal(Calendar::default());
    let retry = signal(None);
    let supported = environment.community_clock().is_some();
    effect(move || {
        let Some(account) = account.get() else { return };
        let Some(clock) = environment.community_clock() else {
            return;
        };
        retry.set(None);
        calendar.set(Calendar {
            day: clock.day,
            ..Calendar::default()
        });
        spawn(run(environment.clone(), account, clock, calendar, retry));
    });
    when(
        move || supported && account.get().is_some(),
        move || {
            view()
                .gap_2()
                .child(text(move || {
                    account.get().map_or_else(String::new, |saved| {
                        format!("签到账号：{}", saved.account_id)
                    })
                }))
                .child(text("社区签到奖励日历").text_size(px(18.0)))
                .child(text("北京时间"))
                .child(dynamic(move || {
                    calendar.with(|calendar| {
                        calendar
                            .rows()
                            .map(|row| text(row).into_element())
                            .collect::<Vec<_>>()
                    })
                }))
                .child(text(move || calendar.with(Calendar::status_label)))
                .child(dynamic(move || {
                    calendar
                        .with(|calendar| calendar.retry_label().map(|label| (calendar.day, label)))
                        .map(|(day, label)| {
                            button(label, move || retry.set(Some(day))).into_element()
                        })
                        .into_iter()
                        .collect::<Vec<_>>()
                }))
        },
    )
}

async fn run(
    environment: Rc<dyn LauncherEnvironment>,
    account: VerifiedAccount,
    mut previous: Clock,
    calendar: Signal<Calendar>,
    retry: Signal<Option<u64>>,
) {
    let mut check = true;
    loop {
        if check {
            let completed = calendar.with_untracked(|calendar| match calendar.today() {
                Status::Complete(result) => Some(*result),
                _ => None,
            });
            if completed.is_none() {
                calendar.update(|calendar| calendar.record(Status::Pending));
            }
            let result = attempt(
                environment.as_ref(),
                &account,
                previous.day,
                completed.is_none(),
            )
            .await;
            if let Some(result) = result
                && (matches!(result, Err(Failure::Authorization | Failure::Verification))
                    || environment
                        .community_clock()
                        .is_some_and(|now| now.day == previous.day))
            {
                match (result, completed) {
                    (Ok(mut result), previous) => {
                        if let Some(previous) = previous {
                            if result.completion == Completion::AlreadySigned {
                                result.completion = previous.completion;
                            }
                            result.reward_claimed |= previous.reward_claimed;
                            result.reward_points = previous.reward_points.or(result.reward_points);
                        }
                        calendar.update(|calendar| calendar.record(Status::Complete(result)));
                    }
                    (Err(Failure::Timeout | Failure::Other(_)), Some(_)) => {}
                    (Err(error), _) => {
                        calendar.update(|calendar| calendar.record(Status::Failed(error)));
                    }
                }
            }
        }

        timer(Duration::from_secs(1)).await;
        let Some(now) = environment.community_clock() else {
            return;
        };
        let new_day = now.day != previous.day;
        if new_day {
            calendar.update(|calendar| calendar.advance(now.day));
        }
        let resumed = now.suspended.saturating_sub(previous.suspended) >= Duration::from_secs(1);
        if resumed {
            previous.suspended = now.suspended;
        }
        previous.day = now.day;
        let requested = retry.get_untracked() == Some(now.day);
        retry.set(None);
        check = calendar.with_untracked(|calendar| {
            let manual = requested && calendar.retry_label().is_some();
            match calendar.blocked {
                Some(Failure::Authorization) => false,
                Some(Failure::Verification) => manual,
                _ => new_day || resumed || manual || matches!(calendar.today(), Status::Pending),
            }
        });
    }
}

async fn attempt(
    environment: &dyn LauncherEnvironment,
    account: &VerifiedAccount,
    day: u64,
    allow_submission: bool,
) -> Option<Result<SignInResult, Failure>> {
    for delay in [1, 2] {
        let result = environment
            .community_sign_in(account.clone(), day, allow_submission)
            .await;
        if result != Err(Failure::Timeout) {
            return Some(result);
        }
        timer(Duration::from_secs(delay)).await;
        if environment.community_clock()?.day != day {
            return None;
        }
    }
    Some(
        environment
            .community_sign_in(account.clone(), day, allow_submission)
            .await,
    )
}
