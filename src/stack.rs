use std::cell::Cell;

const RESERVE: usize = 512 << 10;
const FALLBACK_SIZE: usize = 8 << 20;

thread_local! {
    static LIMIT: Cell<usize> = const { Cell::new(0) };
}

fn address() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

fn install_limit(stack_size: usize, reserve: usize) {
    LIMIT.with(|limit| limit.set(address().saturating_sub(stack_size.saturating_sub(reserve))));
}

fn main_stack_size() -> usize {
    let mut limit: libc::rlimit = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrlimit(libc::RLIMIT_STACK, &mut limit) } != 0
        || limit.rlim_cur == libc::RLIM_INFINITY
    {
        return FALLBACK_SIZE;
    }
    usize::try_from(limit.rlim_cur).unwrap_or(FALLBACK_SIZE)
}

pub fn init() {
    install_limit(main_stack_size(), RESERVE);
}

pub fn exhausted() -> bool {
    let limit = LIMIT.with(Cell::get);
    limit != 0 && address() < limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_without_a_limit_are_never_exhausted() {
        assert!(!exhausted());
    }

    #[test]
    fn deep_recursion_is_detected_before_overflow() {
        fn descend(depth: usize) -> usize {
            if exhausted() {
                return depth;
            }
            let padding = std::hint::black_box([0u8; 4096]);
            descend(depth + 1) + usize::from(padding[0])
        }
        let depth = std::thread::Builder::new()
            .stack_size(32 << 20)
            .spawn(|| {
                install_limit(32 << 20, 4 << 20);
                descend(0)
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(depth > 1000);
        assert!(depth < 32 << 10);
    }
}
