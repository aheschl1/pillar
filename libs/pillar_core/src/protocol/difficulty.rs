use crate::protocol::reputation::N_TRANSMISSION_SIGNATURES;

const INITIAL_BLOCK_REWARD: u64 = 10_000;

/// get more difficult after every 500 blocks
/// the schedule is 4 + 2*(depth // 500)
pub fn get_base_difficulty_from_depth(depth: u64) -> u64{
    if depth == 0{
        return 0; // genesis block
    }
    4+2*(depth/500)
}

/// Get the reward to pay to the miner
/// INITIAL_BLOCK_REWARD/sqrt(x) is the initial reward. This reduces as you have fewer stampers.
/// so, we can do INITIAL_BLOCK_REWARD/sqrt(x) * (N_TRANSMISSION_SIGNATURES/n_stampers). if n_stampers is 0, then no reward
pub fn get_reward_from_depth_and_stampers(depth: u64, n_stampers: usize) -> u64{
    if depth == 0 {
        panic!("Depth cannot be 0 for reward calculation");
    }
    if n_stampers == 0 {
        0
    } else {
        (INITIAL_BLOCK_REWARD * n_stampers as u64) / ((depth as f64).sqrt().floor() as u64 * N_TRANSMISSION_SIGNATURES as u64)
    }

}

#[cfg(test)]
mod test{
    use crate::protocol::{difficulty::{get_base_difficulty_from_depth, get_reward_from_depth_and_stampers, INITIAL_BLOCK_REWARD}, reputation::N_TRANSMISSION_SIGNATURES};

    #[test]
    fn test_initial(){
        for i in 1..499{
            assert!(get_base_difficulty_from_depth(i) == 4);
        }
    }

    #[test]
    fn test_second(){
        for i in 500..999{
            assert!(get_base_difficulty_from_depth(i) == 6)
        }
    }

    #[test]
    fn test_reward_initial(){
        assert_eq!(get_reward_from_depth_and_stampers(1, N_TRANSMISSION_SIGNATURES), INITIAL_BLOCK_REWARD);
        assert_eq!(get_reward_from_depth_and_stampers(1, 0), 0);
        assert!(get_reward_from_depth_and_stampers(1, 1) < INITIAL_BLOCK_REWARD);
    }
}