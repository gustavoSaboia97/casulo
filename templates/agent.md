+++
name = "{name}"
# When the main agent should hand work to this subagent. Required.
description = "Describe when to use {name}"

# The model in each harness, optional. Left out, the subagent uses the
# session model. Names are passed through as written.
# [model]
# claude = "haiku"
# codex = { model = "gpt-5-codex-mini", effort = "low" }
+++

Write the instructions for {name} here, in Markdown.
