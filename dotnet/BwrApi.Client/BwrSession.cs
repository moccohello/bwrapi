using System.Runtime.InteropServices;
using BwrApi.Native;

namespace BwrApi.Client;

/// <summary>Failure of a bwrapi call, with the native status and message.</summary>
public sealed class BwrException(BwrStatus status, string message) : Exception(message)
{
    /// <summary>The status the native call returned.</summary>
    public BwrStatus Status { get; } = status;
}

/// <summary>
/// A connection to one running game. Without the in-game DLL, reads go
/// straight to the game's memory and nothing is injected. Use a session from
/// one thread at a time.
/// </summary>
public sealed class BwrSession : IDisposable
{
    private nint handle;

    private BwrSession(nint handle) => this.handle = handle;

    /// <summary>Opens a session on the running game with process id <paramref name="processId"/>.</summary>
    /// <exception cref="BwrException">The process cannot be opened or is not a supported game build.</exception>
    public static BwrSession Open(uint processId)
    {
        BwrStatus status = BwrNative.Open(processId, 0, out nint handle);
        if (status != BwrStatus.Ok)
            throw new BwrException(status, Marshal.PtrToStringUTF8(BwrNative.LastError(0)) ?? status.ToString());
        return new BwrSession(handle);
    }

    /// <summary>Selects the sources <see cref="TryAcquire"/> reads.</summary>
    public void Subscribe(BwrSource sources) => BwrNative.Subscribe(Handle, sources);

    /// <summary>
    /// Waits up to <paramref name="timeoutMs"/> milliseconds for a game frame
    /// newer than the last acquired one (or a new match). Returns false on
    /// timeout, and at once when no match is running; throws when the game
    /// process is gone.
    /// </summary>
    public bool Step(uint timeoutMs)
    {
        BwrStatus status = BwrNative.Step(Handle, timeoutMs);
        if (status is BwrStatus.Timeout or BwrStatus.NotInGame)
            return false;
        if (status != BwrStatus.Ok)
            throw new BwrException(status, Marshal.PtrToStringUTF8(BwrNative.LastError(Handle)) ?? status.ToString());
        return true;
    }

    /// <summary>
    /// Reads the subscribed sources. Returns false when no match is running;
    /// throws when the game process is gone.
    /// </summary>
    public unsafe bool TryAcquire(out BwrFrameLease? lease)
    {
        lease = null;
        BwrStatus status = BwrNative.Acquire(Handle, out BwrFrame* frame);
        if (status == BwrStatus.NotInGame)
            return false;
        if (status != BwrStatus.Ok)
            throw new BwrException(status, Marshal.PtrToStringUTF8(BwrNative.LastError(Handle)) ?? status.ToString());
        lease = new BwrFrameLease(Handle, frame);
        return true;
    }

    /// <summary>The native session handle, for <see cref="BwrNative"/> calls this class does not wrap.</summary>
    public nint Handle => handle != 0 ? handle : throw new ObjectDisposedException(nameof(BwrSession));

    /// <summary>Closes the session.</summary>
    public void Dispose()
    {
        BwrNative.Close(handle);
        handle = 0;
    }
}

/// <summary>
/// One frame. Its tables and column spans stay valid until the lease is
/// disposed; a lease may be disposed on any thread.
/// </summary>
public sealed unsafe class BwrFrameLease : IDisposable
{
    private readonly nint session;
    private BwrFrame* frame;

    internal BwrFrameLease(nint session, BwrFrame* frame)
    {
        this.session = session;
        this.frame = frame;
    }

    /// <summary>The frame header and its tables; a table is null when the frame does not carry its source.</summary>
    public ref readonly BwrFrame Frame
    {
        get
        {
            ObjectDisposedException.ThrowIf(frame == null, this);
            return ref *frame;
        }
    }

    /// <summary>Releases the frame.</summary>
    public void Dispose()
    {
        BwrNative.Release(session, frame);
        frame = null;
    }
}
