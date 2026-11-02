# BwrApi.Client

.NET binding of the BWRAPI C ABI 1.0. BWRAPI is BWAPI for StarCraft: Remastered 1.23.10 (x64 and x86 games). This package reads a running game as column tables, one table per source (units, players, bullets, terrain and more), without injecting anything. The package carries `bwrapi.dll` for x64 and x86 .NET processes (`runtimes/win-x64`, `runtimes/win-x86`); Windows only, .NET 8 or later.

## Install

```
dotnet add package BwrApi.Client
```

## Example

Prints the local player's units on every new game frame of the game whose process id is given on the command line:

```csharp
using BwrApi.Client;
using BwrApi.Native;

uint processId = uint.Parse(args[0]);
using BwrSession session = BwrSession.Open(processId);
session.Subscribe(BwrSource.Units | BwrSource.Players);
try
{
    while (true)
    {
        if (!session.Step(1000))
        {
            Thread.Sleep(100); // no match running, or no new frame yet
            continue;
        }
        if (!session.TryAcquire(out BwrFrameLease? lease))
            continue;
        using (lease)
        {
            ref readonly BwrFrame frame = ref lease!.Frame;
            BwrUnits units = frame.Units!.Value;
            for (int row = 0; row < (int)units.Rows; row++)
            {
                if (units.Owner[row] == frame.LocalPlayer)
                    Console.WriteLine($"{frame.GameFrame}: {units.Id[row]} type {units.UnitType[row]} at {units.X[row]},{units.Y[row]}");
            }
        }
    }
}
catch (BwrException e)
{
    Console.WriteLine($"Stopped: {e.Message}"); // the game process is gone
}
```

- `Step` waits for a newer game frame; it returns false on timeout and at once when no match is running. `TryAcquire` returns false when no match is running. Both throw `BwrException` when the game process is gone, so catch it to end the loop cleanly.
- Tables and column spans stay valid until the lease is disposed. Use a session from one thread at a time.
- `frame.Flags` is the enum `BwrFrameFlags` (`Torn`, `Replay`). Text columns are `BwrText32` (and `BwrText256`); `ToString()` decodes UTF-8. A name the game keeps in its legacy code page (a Korean client's computer names are CP949) comes out garbled that way. A text value converts implicitly to `ReadOnlySpan<byte>` (it is an inline array of bytes): take the bytes up to the first NUL and decode them yourself, for example `ReadOnlySpan<byte> raw = players.Name[row];` with `Encoding.GetEncoding(949)` after `Encoding.RegisterProvider(CodePagesEncodingProvider.Instance)`.
- Unknown values are the type's sentinel (the maximum for unsigned types, the minimum for signed ones). A unit id is `(slot + 1) | uniqueness << 16`; 0 is no unit.
- The other native calls (`BwrNative.Submit` and `PollResult` for commands, chat capture, drawing) take `session.Handle`. Commands need the in-game DLL (`bwrapi.dll` injected into the game); drawing and chat need a module running inside the game.

## Documentation

- C ABI reference (functions, status codes, every source and column with its units): https://github.com/moccohello/bwrapi/blob/main/docs/C-ABI.md
- BWRAPI: https://github.com/moccohello/bwrapi
