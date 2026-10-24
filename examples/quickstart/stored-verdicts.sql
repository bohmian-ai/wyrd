-- Replace <uid> with the Service's metadata.uid from wyrd get.
SELECT result_id, run_id, card_uid, verdict FROM bifrost.verification.results WHERE card_uid = '<uid>' ORDER BY ended_at DESC LIMIT 20
