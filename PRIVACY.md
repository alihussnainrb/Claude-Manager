# Privacy policy

Claude Manager collects no personal data and sends nothing about you anywhere.

- **No telemetry, analytics or crash reporting.** The app makes no network requests of its own,
  except that the GitHub download build checks GitHub Releases for updates (you can turn this off).
  The Microsoft Store build gets updates from the Store and makes no requests at all.
- **Everything stays on your PC.** Profiles and settings are stored in `%APPDATA%\ClaudeManager`.
  Each profile's Claude data lives in its own folder under `%APPDATA%`.
- **Sign-in links** (`claude://`) are handed straight to the Claude profile that asked for them.
  They are never stored or transmitted, and logs drop their query part, which carries codes and state.
- **Credentials are never read.** The app reads only the account ID Claude stores in each profile, to
  warn when two profiles are signed into the same account.

Claude itself is run by Anthropic and covered by [Anthropic's privacy policy](https://www.anthropic.com/legal/privacy).

Questions: open an issue at https://github.com/alihussnainrb/Claude-Manager/issues.
