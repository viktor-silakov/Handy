use std::os::raw::{c_char, c_int};

// Define the response structure from Swift
#[allow(dead_code)]
#[repr(C)]
pub struct AppleLLMResponse {
    pub response: *mut c_char,
    pub success: c_int,
    pub error_message: *mut c_char,
}

// Link to the Swift functions (commented out)
// extern "C" {
//     pub fn is_apple_intelligence_available() -> c_int;
//     pub fn free_apple_llm_response(response: *mut AppleLLMResponse);
// }

// Safe wrapper functions
pub fn check_apple_intelligence_availability() -> bool {
    // Forced false since Swift bridge is disabled
    false
}

// Link to the Swift function for system prompt support (commented out)
// extern "C" {
//     pub fn process_text_with_system_prompt_apple(
//         system_prompt: *const c_char,
//         user_content: *const c_char,
//         max_tokens: i32,
//     ) -> *mut AppleLLMResponse;
// }

/// Process text with Apple Intelligence using separate system prompt and user content
pub fn process_text_with_system_prompt(
    _system_prompt: &str,
    _user_content: &str,
    _max_tokens: i32,
) -> Result<String, String> {
    Err("Apple Intelligence is disabled on this build.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_availability() {
        let available = check_apple_intelligence_availability();
        println!("Apple Intelligence available: {}", available);
    }
}
