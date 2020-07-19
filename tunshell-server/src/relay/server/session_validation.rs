use crate::db::Session;
use chrono::Utc;

pub(super) fn is_session_valid_to_join(session: &Session, key: &str) -> bool {
    // Ensure session is not older than a day
    if Utc::now() - session.created_at > chrono::Duration::days(1) {
        return false;
    }

    let participant = session.participant(key);

    if participant.is_none() {
        return false;
    }

    return true;
}
