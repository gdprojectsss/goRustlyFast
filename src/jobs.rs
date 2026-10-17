use std::collections::HashMap;

use crate::sys::{self, Pid, WaitResult};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub pgid: Pid,
    pub pids: Vec<Pid>,
    pub results: Vec<Option<WaitResult>>,
    pub text: String,
    pub state: JobState,
    pub keep_on_hangup: bool,
}

impl Job {
    pub fn running_pids(&self) -> Vec<Pid> {
        self.pids
            .iter()
            .zip(&self.results)
            .filter(|(_, result)| result.is_none())
            .map(|(&pid, _)| pid)
            .collect()
    }

    pub fn status(&self) -> i32 {
        self.results
            .last()
            .copied()
            .flatten()
            .map_or(0, WaitResult::status)
    }

    pub fn state_text(&self) -> String {
        match self.state {
            JobState::Running => "Running".into(),
            JobState::Stopped => "Stopped".into(),
            JobState::Done => match self.results.last().copied().flatten() {
                Some(WaitResult::Exited(0)) | None => "Done".into(),
                Some(WaitResult::Exited(code)) => format!("Exit {code}"),
                Some(WaitResult::Signaled(signal)) => sys::signal_description(signal),
                Some(_) => "Done".into(),
            },
        }
    }

    pub fn line(&self, marker: char, with_pid: bool) -> String {
        let pid = if with_pid {
            format!("{:>5} ", self.pgid)
        } else {
            " ".to_string()
        };
        let suffix = if self.state == JobState::Running {
            " &"
        } else {
            ""
        };
        format!(
            "[{}]{marker} {pid}{:<27}{}{suffix}",
            self.id,
            self.state_text(),
            self.text
        )
    }

    fn record(&mut self, pid: Pid, result: WaitResult) -> bool {
        let Some(position) = self.pids.iter().position(|&known| known == pid) else {
            return false;
        };
        match result {
            WaitResult::Stopped => self.state = JobState::Stopped,
            WaitResult::Continued => self.state = JobState::Running,
            finished => {
                self.results[position] = Some(finished);
                if self.results.iter().all(Option::is_some) {
                    self.state = JobState::Done;
                }
            }
        }
        true
    }
}

#[derive(Default)]
pub struct JobTable {
    jobs: Vec<Job>,
    remembered: HashMap<Pid, i32>,
    inherited: Vec<Job>,
}

impl JobTable {
    pub fn add(
        &mut self,
        id: Option<usize>,
        pgid: Pid,
        pids: Vec<Pid>,
        text: &str,
        state: JobState,
    ) -> usize {
        let id = id.unwrap_or_else(|| self.jobs.iter().map(|job| job.id).max().unwrap_or(0) + 1);
        for pid in &pids {
            self.remembered.remove(pid);
        }
        let results = vec![None; pids.len()];
        let job = Job {
            id,
            pgid,
            pids,
            results,
            text: text.to_string(),
            state,
            keep_on_hangup: false,
        };
        let position = self.jobs.partition_point(|existing| existing.id < id);
        self.jobs.insert(position, job);
        id
    }

    pub fn clear(&mut self) {
        self.jobs.clear();
        self.remembered.clear();
        self.inherited.clear();
    }

    pub fn inherit(&mut self) {
        let mut jobs = std::mem::take(&mut self.jobs);
        jobs.retain(|job| job.state != JobState::Done);
        self.clear();
        self.inherited = jobs;
    }

    pub fn inherited(&self) -> &[Job] {
        &self.inherited
    }

    pub fn len(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| job.state != JobState::Done)
            .count()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter()
    }

    pub fn current_id(&self) -> Option<usize> {
        self.jobs.last().map(|job| job.id)
    }

    pub fn previous_id(&self) -> Option<usize> {
        self.jobs.iter().rev().nth(1).map(|job| job.id)
    }

    pub fn marker(&self, id: usize) -> char {
        if Some(id) == self.current_id() {
            '+'
        } else if Some(id) == self.previous_id() {
            '-'
        } else {
            ' '
        }
    }

    pub fn find(&self, spec: Option<&str>) -> Result<usize, String> {
        let spec = spec.map(|text| text.strip_prefix('%').unwrap_or(text));
        let found = match spec {
            None | Some("") | Some("+") | Some("%") => self.jobs.len().checked_sub(1),
            Some("-") => self
                .jobs
                .len()
                .checked_sub(2)
                .or_else(|| self.jobs.len().checked_sub(1)),
            Some(text) => match text.parse::<usize>() {
                Ok(number) => self.jobs.iter().position(|job| job.id == number),
                Err(_) => match text.strip_prefix('?') {
                    Some(inner) => self.jobs.iter().rposition(|job| job.text.contains(inner)),
                    None => self.jobs.iter().rposition(|job| job.text.starts_with(text)),
                },
            },
        };
        found.ok_or_else(|| match spec {
            None => "no current job".to_string(),
            Some(text) => format!("%{text}: no such job"),
        })
    }

    pub fn index_of(&self, id: usize) -> Option<usize> {
        self.jobs.iter().position(|job| job.id == id)
    }

    pub fn first_done(&self) -> Option<usize> {
        self.jobs.iter().position(|job| job.state == JobState::Done)
    }

    pub fn find_pid(&self, pid: Pid) -> Option<usize> {
        self.jobs.iter().position(|job| job.pids.contains(&pid))
    }

    pub fn get(&self, index: usize) -> &Job {
        &self.jobs[index]
    }

    pub fn get_mut(&mut self, index: usize) -> &mut Job {
        &mut self.jobs[index]
    }

    pub fn remove(&mut self, index: usize) -> Job {
        self.jobs.remove(index)
    }

    pub fn forget(&mut self, index: usize) -> Job {
        let job = self.jobs.remove(index);
        for (pid, result) in job.pids.iter().zip(&job.results) {
            if let Some(result) = result {
                self.remembered.insert(*pid, result.status());
            }
        }
        job
    }

    pub fn remembered_status(&self, pid: Pid) -> Option<i32> {
        self.remembered.get(&pid).copied()
    }

    pub fn has_running(&self) -> bool {
        self.jobs.iter().any(|job| job.state != JobState::Done)
    }

    pub fn reap(&mut self) {
        for job in &mut self.jobs {
            if job.state == JobState::Done {
                continue;
            }
            for pid in job.running_pids() {
                match sys::waitpid(pid, libc::WNOHANG | libc::WUNTRACED | libc::WCONTINUED) {
                    Ok(None) => {}
                    Ok(Some(result)) => {
                        job.record(pid, result);
                    }
                    Err(_) => {
                        job.record(pid, WaitResult::Exited(0));
                    }
                }
            }
        }
    }

    pub fn take_finished(&mut self) -> Vec<String> {
        let mut notices = Vec::new();
        while let Some(index) = self.first_done() {
            let marker = self.marker(self.jobs[index].id);
            let job = self.forget(index);
            notices.push(job.line(marker, false));
        }
        notices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> JobTable {
        let mut table = JobTable::default();
        table.add(None, 100, vec![100], "sleep 10", JobState::Running);
        table.add(None, 200, vec![200], "vim notes", JobState::Stopped);
        table
    }

    #[test]
    fn ids_increase() {
        let table = table();
        let ids: Vec<_> = table.iter().map(|job| job.id).collect();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn job_specs() {
        let table = table();
        assert_eq!(table.find(None), Ok(1));
        assert_eq!(table.find(Some("%1")), Ok(0));
        assert_eq!(table.find(Some("%-")), Ok(0));
        assert_eq!(table.find(Some("%vim")), Ok(1));
        assert_eq!(table.find(Some("%?notes")), Ok(1));
        assert!(table.find(Some("%9")).is_err());
        assert!(JobTable::default().find(None).is_err());
    }

    #[test]
    fn reused_id_keeps_order() {
        let mut table = table();
        let job = table.remove(0);
        table.add(
            Some(job.id),
            job.pgid,
            job.pids,
            &job.text,
            JobState::Stopped,
        );
        assert_eq!(table.iter().map(|job| job.id).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn finished_jobs_are_remembered() {
        let mut table = table();
        assert!(table.get_mut(0).record(100, WaitResult::Exited(3)));
        assert_eq!(table.get(0).state, JobState::Done);
        assert_eq!(table.len(), 1);
        let notices = table.take_finished();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].starts_with("[1]-  Exit 3"));
        assert_eq!(table.remembered_status(100), Some(3));
        assert!(!table.get_mut(0).record(999, WaitResult::Exited(0)));
    }

    #[test]
    fn lines_match_bash_layout() {
        let table = table();
        assert_eq!(
            table.get(0).line('-', false),
            format!("[1]-  {:<27}sleep 10 &", "Running")
        );
        assert_eq!(
            table.get(1).line('+', true),
            format!("[2]+   200 {:<27}vim notes", "Stopped")
        );
    }
}
