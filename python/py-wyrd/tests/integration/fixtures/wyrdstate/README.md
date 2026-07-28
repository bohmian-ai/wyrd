This directory reserves the Python journey fixture namespace. The deterministic
Card source is shared with the committed CLI typed-state fixture; tests copy it
to a temporary directory before registration so dynamic UIDs never mutate
repository files.
