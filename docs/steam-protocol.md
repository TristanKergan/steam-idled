# Steam Game Presence & Client Protocol Analysis

This document explains the technical mechanisms behind Steam game presence, how games communicate with Steam on Linux, and why this implementation was chosen.

---

## 1. Network Protocol vs. Local IPC

Steam provides two main ways to establish game presence:

### Method A: Steam CM Network Protocol (SteamKit / Protobuf)
- **Mechanism**: The client establishes a direct TCP/WebSocket connection with Valve's Connection Manager (CM) servers.
- **Message**: Protobuf message `EMsg::ClientGamesPlayed` (`CMsgClientGamesPlayed` defined in `steammessages_clientserver.proto`).
- **Payload**:
  ```protobuf
  message CMsgClientGamesPlayed {
      message GamePlayed {
          uint64 game_id = 1;
          uint32 process_id = 2;
          string game_extra_info = 3;
          bool is_secure = 4;
      }
      repeated GamePlayed games_played = 1;
  }
  ```
- **Trade-off**: Requires full user login credentials (username + password / Steam Guard / refresh token). Logging in through this method either conflicts with or disconnects the desktop client session, requiring credential storage which violates security requirements.

### Method B: Local Steam Client IPC (Steamworks SDK / `libsteam_api.so`)
- **Mechanism**: The local Steam desktop client runs on the user's desktop with an active, authenticated session. Applications connect to the client over local IPC (`~/.steam/steam.pipe` or IPC shared memory `/tmp/steam_chrome_shmem_*`).
- **Initialization**:
  - The process exports `SteamAppId=<APPID>` and calls `SteamAPI_Init()` / `SteamAPI_InitFlat()`.
  - The local Steam client recognizes that a game session has started.
  - The local Steam client automatically constructs and transmits `CMsgClientGamesPlayed` to Valve's CM servers on behalf of the user.
  - Playtime accumulates in the user's profile and friends list shows "In-Game".
- **Clearing Presence**: Calling `SteamAPI_Shutdown()` or terminating the runner process closes the IPC channel. The Steam client immediately removes the game from `CMsgClientGamesPlayed` and broadcasts the update to Valve servers.
- **Why this was chosen**:
  - **Zero Credentials**: Uses the existing authenticated Steam session.
  - **Headless & Safe**: No game assets or executables are downloaded or executed.
  - **Seamless**: Desktop client UI accurately reflects game state.

---

## 2. Multi-Game Support

While `libsteam_api.so` uses process-global static state for a single AppID per process, running multiple worker processes concurrently (`steam-idled worker --appid <id>`) allows idling multiple games at once.

The Steam desktop client natively aggregates active game sessions from multiple local processes and packages them into `CMsgClientGamesPlayed.games_played`, broadcasting all active games to Valve.

---

## 3. Real Game Launch Handling (`auto_resume`)

When a user attempts to launch the real CS2 game while idle presence is active, Steam may report that the application is already running.

`steam-idled` solves this via its **Process Detector**:
1. Daemon monitors `/proc` for processes named `cs2`.
2. When the real `cs2` binary starts, `steam-idled` terminates the idle worker process, calling `SteamAPI_Shutdown()`.
3. Steam client clears idle presence, allowing the real CS2 game to launch cleanly.
4. When the user finishes playing and closes CS2, `steam-idled` waits for a debounce interval (default: 10s) and automatically restarts the idle worker process.

---

## 4. References & Sources

1. **Valve Steamworks API Documentation**: [partner.steamgames.com/doc/api/steam_api](https://partner.steamgames.com/doc/api/steam_api)
2. **SteamKit2 CMsgClientGamesPlayed Specification**: `SteamKit2/SteamKit/src/Generated/SteamLanguage/steammessages_clientserver.cs`
3. **Steam Database (SteamDB)**: Protocol and Protobuf Definitions: [github.com/SteamDatabase/Protobufs](https://github.com/SteamDatabase/Protobufs)
4. **Rust Steamworks Bindings**: [docs.rs/steamworks](https://docs.rs/steamworks)
