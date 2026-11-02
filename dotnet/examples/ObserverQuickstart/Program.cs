using BwrApi.Client;
using BwrApi.Native;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: ObserverQuickstart <game process id>");
    return 2;
}
uint processId = uint.Parse(args[0]);

using BwrSession session = BwrSession.Open(processId);
session.Subscribe(BwrSource.Players | BwrSource.Units | BwrSource.Viewport);
if (!session.TryAcquire(out BwrFrameLease? lease))
{
    Console.WriteLine("not in game");
    return 1;
}
using (lease)
{
    ref readonly BwrFrame frame = ref lease!.Frame;
    BwrUnits units = frame.Units!.Value;
    BwrViewport? viewport = frame.Viewport;
    Console.WriteLine($"frame {frame.GameFrame} match {frame.MatchId} local {frame.LocalPlayer} " +
        $"players {frame.Players?.Rows} units {units.Rows} viewport {viewport?.X[0]},{viewport?.Y[0]}");
    for (int row = 0; row < (int)units.Rows; row++)
    {
        if (units.Owner[row] == frame.LocalPlayer)
            Console.WriteLine($"  {units.Id[row]} type {units.UnitType[row]} at {units.X[row]},{units.Y[row]}");
    }
}
return 0;
