-- Replace <uid> with the Service's metadata.uid from wyrd get.
SELECT result_id, run_id, subject_card_uid, verdict FROM bifrost.verification.results WHERE subject_card_uid = '<uid>' ORDER BY ended_at DESC LIMIT 20
