use crate::sys::{self, Pid, WaitResult};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Running,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub pgid: Pid,
    pub pids: Vec<Pid>,
    pub text: String,
    pub state: JobState,
}

#[derive(Default)]
pub struct JobTable {
    jobs: Vec<Job>,
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
        self.jobs.push(Job {
            id,
            pgid,
            pids,
            text: text.to_string(),
            state,
        });
        self.jobs.sort_by_key(|job| job.id);
        id
    }

    pub fn clear(&mut self) {
        self.jobs.clear();
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter()
    }

    pub fn current_id(&self) -> Option<usize> {
        self.jobs.last().map(|job| job.id)
    }

    pub fn find(&self, spec: Option<&str>) -> Result<usize, String> {
        let spec = spec.map(|text| text.strip_prefix('%').unwrap_or(text));
        let found = match spec {
            None | Some("") | Some("+") | Some("%") => self.jobs.len().checked_sub(1),
            Some("-") => self.jobs.len().checked_sub(2),
            Some(text) => match text.parse::<usize>() {
                Ok(number) => self.jobs.iter().position(|job| job.id == number),
                Err(_) => self.jobs.iter().rposition(|job| job.text.starts_with(text)),
            },
        };
        found.ok_or_else(|| match spec {
            None => "no current job".to_string(),
            Some(text) => format!("%{text}: no such job"),
        })
    }

    pub fn get_mut(&mut self, index: usize) -> &mut Job {
        &mut self.jobs[index]
    }

    pub fn remove(&mut self, index: usize) -> Job {
        self.jobs.remove(index)
    }

    pub fn reap(&mut self) -> Vec<String> {
        let mut notices = Vec::new();
        let current = self.current_id();
        for job in &mut self.jobs {
            let Job { pids, state, .. } = job;
            let mut last_result = WaitResult::Exited(0);
            pids.retain(|&pid| {
                match sys::waitpid(pid, libc::WNOHANG | libc::WUNTRACED | libc::WCONTINUED) {
                    Ok(None) => true,
                    Ok(Some(WaitResult::Stopped)) => {
                        *state = JobState::Stopped;
                        true
                    }
                    Ok(Some(WaitResult::Continued)) => {
                        *state = JobState::Running;
                        true
                    }
                    Ok(Some(result)) => {
                        last_result = result;
                        false
                    }
                    Err(_) => false,
                }
            });
            if job.pids.is_empty() {
                let marker = if Some(job.id) == current { '+' } else { ' ' };
                let what = finished_description(last_result);
                notices.push(format!("[{}]{marker}  {what:<10} {}", job.id, job.text));
            }
        }
        self.jobs.retain(|job| !job.pids.is_empty());
        notices
    }
}

pub fn finished_description(result: WaitResult) -> String {
    match result {
        WaitResult::Exited(0) => "Done".to_string(),
        WaitResult::Exited(code) => format!("Exit {code}"),
        WaitResult::Signaled(signal) => sys::signal_description(signal),
        WaitResult::Stopped => "Stopped".to_string(),
        WaitResult::Continued => "Running".to_string(),
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
        assert!(table.find(Some("%9")).is_err());
        assert!(JobTable::default().find(None).is_err());
    }

    #[test]
    fn finished_jobs_are_described_like_bash() {
        assert_eq!(finished_description(WaitResult::Exited(0)), "Done");
        assert_eq!(finished_description(WaitResult::Exited(3)), "Exit 3");
        assert!(finished_description(WaitResult::Signaled(libc::SIGKILL)).starts_with("Killed"));
        assert!(
            finished_description(WaitResult::Signaled(libc::SIGTERM)).starts_with("Terminated")
        );
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
}
