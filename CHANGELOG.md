# v13 (2020/10/04)

## Features

- Update to buttplug-rs 0.9.0, which now has Battery level reading capabilites
  for some hardware.

## Bugfixes

- Update to buttplug-rs 0.9.0, which now does not crash when 2 devices are
  connected and one disconnects.

# v12 (2020/10/02)

## Features

- Update to Buttplug-rs 0.8.4, fixing a bunch of device issues.
- Default to outputting info level logs if no env log var set. (Should pick this
  up from command line argument in future version)

## Bugfixes

- Only run for one connection attempt if --stayopen isn't passed in.

# v11 (2020/09/20)

## Bugfixes

- Moves to buttplug-0.8.3, which fixes support for some programs using older
  APIs (FleshlightLaunchFW12Cmd) for Kiiroo stroking products (Onyx, Fleshlight
  Launch, etc).

# v10 (2020/09/13)

## Features

- Added log handling from Buttplug library. Still needs protocol/CLI setting,
  currently outputs everything INFO or higher.

## Bugfixes

- Moves to buttplug-0.8.2, fixing Lovense rotation and adding log output
  support.

# v9 (2020/09/11)

## Bugfixes

- Moves to buttplug-0.7.3, which loads both RSA and pkcs8 certificates. This
  allows us to load the certs that come from Intiface Desktop.

# v8 (2020/09/07)

## Bugfixes

- Move to buttplug-rs 0.7.2, which adds more device configurations and fixes
  websocket listening on all interfaces.

# v7 (2020/09/06)

## Features

- Move to buttplug-rs 0.7.1, which includes status emitting features and way
  more device protocol support.
- Allow frontend to trigger process stop
- Send disconnect to frontend when client disconnects
- Can now relay connected/disconnected devices to GUIs via PBuf protocol

# v6 (2020/08/06)

## Features

- Move to buttplug-rs 0.6.0, which integrates websockets and server lifetime
  handling. intiface-cli-rs is now a very thin wrapper around buttplug-rs,
  handling system bringup and frontend communication and that's about it.

# v5 (2020/05/13)

## Bugfixes

- Move to buttplug-rs 0.3.1, with a couple of unwrap fixes

# v4 (2020/05/10)

## Features

- --stayopen option now actually works, reusing the server between
  client connections.

# v3 (2020/05/09)

## Features

- Added protobuf basis for hooking CLI into Intiface Desktop

## Bugfixes

- Fixed bug where receiving ping message from async_tungstenite would
  panic server
- Update to buttplug 0.2.4, which fixes ServerInfo message ID matching

# v2 (2020/02/15)

## Features

- Move to using rolling versioning, since this is a binary
- Move to using buttplug 0.2, with full server implementation
- Add cert generation
- Add secure websocket capabilities
- Move to using async-tungstenite
- Use Buttplug's built in JSONWrapper
- Add XInput capability on windows
- Add CI building
- Add Simple GUI message output for Intiface Desktop

# v1 (aka v0.0.1) (2020/02/15)

## Features

- First version
- Can bring up insecure websocket, run server, access toys
- Most options not used yet
