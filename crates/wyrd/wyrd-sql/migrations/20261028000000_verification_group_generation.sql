-- Each pending group row carries the generation of its latest acknowledged
-- activity. Every activity upsert draws a fresh one, and a row recreated after
-- consumption starts with a fresh one, so the queue writer that admits an
-- acknowledged group execution consumes the row only when it still holds the
-- generation that candidate was selected from. Newer activity stays pending.
ALTER TABLE wyrd.verification_group_pending
    ADD COLUMN generation UUID NOT NULL DEFAULT gen_random_uuid();
