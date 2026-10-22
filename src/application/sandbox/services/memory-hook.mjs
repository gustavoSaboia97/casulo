// casulo's memory hook: relays one hook of the agent to the memory server
// and prints its answer, which the agent reads. casulo puts the server's
// URL in the fetch below when it generates this file. It fails open: without
// a session, on any error or after 5 seconds it prints nothing, and the agent
// goes on as without it.

// The deadline covers reading stdin, connecting and the answer, and ends the
// process well before the harness gives up on the hook.
setTimeout(() => process.exit(0), 5000);
const session = process.env.CASULO_SESSION;
let answer = "";
if (session) {
  try {
    process.stdin.setEncoding("utf8");
    let body = "";
    for await (const chunk of process.stdin) {
      body += chunk;
    }
    const response = await fetch("__URL__", {
      method: "POST",
      headers: { "content-type": "application/json", "x-casulo-session": session },
      body,
    });
    if (response.ok) {
      answer = await response.text();
    }
  } catch {
    // Fail open: the agent must never be stuck on memory.
  }
}
// Writes to a pipe are synchronous in Node on Linux, so exiting right after
// cannot cut the answer.
process.stdout.write(answer);
process.exit(0);
