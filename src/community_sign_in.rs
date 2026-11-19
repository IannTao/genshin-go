//! Community sign-in for the locally authorized account.
use crate::{
    launcher::{LauncherEnvironment, button},
    profile::{Profile, VerifiedAccount},
};
use nsh_core::{
    element::{computed, effect, timer},
    prelude::*,
};
use std::{future::Future, pin::Pin, rc::Rc, time::Duration};

/// Represents one clock sample for community sign-in.
#[derive(Clone, Copy)]
pub struct Clock {
    /// Identifies the service's current day.
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
pub type Operation = Pin<Box<dyn Future<Output = Result<Completion, Failure>>>>;

#[derive(Clone, PartialEq, Eq)]
enum Status {
    Pending,
    Complete(Completion),
    Failed(Failure),
}

impl Status {
    fn label(&self) -> String {
        match self {
            Self::Pending => "社区签到：待处理",
            Self::Complete(Completion::Signed) => "社区签到：成功",
            Self::Complete(Completion::AlreadySigned) => "社区签到：已签到",
            Self::Failed(Failure::Timeout) => "社区签到：失败，网络超时，请重试",
            Self::Failed(Failure::Authorization) => "社区签到：失败，签到授权不可用，请重新扫码",
            Self::Failed(Failure::Verification) => "社区签到：失败，请在米游社完成验证或签到后重试",
            Self::Failed(Failure::Other(message)) => return format!("社区签到：失败，{message}"),
        }
        .into()
    }

    fn can_retry(&self) -> bool {
        matches!(self, Self::Failed(error) if *error != Failure::Authorization)
    }
}

/// Shows the status of the saved account's community sign-in.
pub(crate) fn controls(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
) -> impl IntoElement {
    let account = computed(move || profile.get().account);
    let status = signal(Status::Pending);
    let retry = signal(false);
    let supported = environment.community_clock().is_some();
    effect(move || {
        let Some(account) = account.get() else { return };
        let Some(clock) = environment.community_clock() else {
            return;
        };
        retry.set(false);
        status.set(Status::Pending);
        spawn(run(environment.clone(), account, clock, status, retry));
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
                .child(text(move || status.with(Status::label)))
                .child(when(
                    move || status.with(Status::can_retry),
                    move || button("重试社区签到", move || retry.set(true)),
                ))
        },
    )
}

async fn run(
    environment: Rc<dyn LauncherEnvironment>,
    account: VerifiedAccount,
    mut previous: Clock,
    status: Signal<Status>,
    retry: Signal<bool>,
) {
    let mut check = true;
    let mut status_day = previous.day;
    loop {
        if check {
            let already_complete = status_day == previous.day
                && status.with_untracked(|current| matches!(current, Status::Complete(_)));
            if !already_complete {
                status.set(Status::Pending);
            }
            status_day = previous.day;
            let result = attempt(environment.as_ref(), &account, previous.day).await;
            if let Some(result) = result
                && (matches!(result, Err(Failure::Authorization | Failure::Verification))
                    || environment
                        .community_clock()
                        .is_some_and(|now| now.day == previous.day))
            {
                match result {
                    Ok(Completion::AlreadySigned) | Err(Failure::Timeout | Failure::Other(_))
                        if already_complete => {}
                    Ok(completion) => status.set(Status::Complete(completion)),
                    Err(error) => status.set(Status::Failed(error)),
                }
            }
        }

        timer(Duration::from_secs(1)).await;
        let Some(now) = environment.community_clock() else {
            return;
        };
        let new_day = now.day != previous.day;
        let resumed = now.suspended.saturating_sub(previous.suspended) >= Duration::from_secs(1);
        if resumed {
            previous.suspended = now.suspended;
        }
        previous.day = now.day;
        let current = status.get_untracked();
        let manual = retry.get_untracked() && current.can_retry();
        retry.set(false);
        check = match current {
            Status::Failed(Failure::Authorization) => false,
            Status::Failed(Failure::Verification) => manual,
            Status::Complete(_) => new_day || resumed,
            Status::Failed(_) => new_day || resumed || manual,
            Status::Pending => true,
        };
    }
}

async fn attempt(
    environment: &dyn LauncherEnvironment,
    account: &VerifiedAccount,
    day: u64,
) -> Option<Result<Completion, Failure>> {
    for delay in [1, 2] {
        let result = environment.community_sign_in(account.clone(), day).await;
        if result != Err(Failure::Timeout) {
            return Some(result);
        }
        timer(Duration::from_secs(delay)).await;
        if environment.community_clock()?.day != day {
            return None;
        }
    }
    Some(environment.community_sign_in(account.clone(), day).await)
}
