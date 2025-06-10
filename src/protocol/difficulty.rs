
/// get more difficult after every 500 blocks
/// the schedule is 4 + 2*(depth // 500)
pub fn get_difficulty_from_depth(depth: u64) -> u64{
    if depth == 0{
        return 0; // genesis block
    }
    4+2*(depth/500)
}

#[cfg(test)]
mod test{
    use crate::protocol::difficulty::get_difficulty_from_depth;

    #[test]
    fn test_initial(){
        for i in 1..499{
            assert!(get_difficulty_from_depth(i) == 4);
        }
    }

    #[test]
    fn test_second(){
        for i in 500..999{
            assert!(get_difficulty_from_depth(i) == 6)
        }
    }
}