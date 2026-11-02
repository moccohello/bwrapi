"""Static BWAPI 4.4 profile information and tournament action IDs."""

from enum import IntEnum

from .profile import BWAPI_IS_DEBUG, BWAPI_REVISION


def BWAPI_getRevision() -> int:
    """Return the BWAPI revision of the Python BWAPI 4.4 profile."""
    return BWAPI_REVISION


def BWAPI_isDebug() -> bool:
    """Return whether this Python production profile is a BWAPI Debug build."""
    return BWAPI_IS_DEBUG


class Tournament:
    """Namespace for BWAPI tournament actions."""

    class ActionID(IntEnum):
        EnableFlag = 0
        PauseGame = 1
        ResumeGame = 2
        LeaveGame = 3
        SetLocalSpeed = 4
        SetTextSize = 5
        SetLatCom = 6
        SetGUI = 7
        SetMap = 8
        SetFrameSkip = 9
        Printf = 10
        SendText = 11
        SetCommandOptimizationLevel = 12

    # TournamentAction.h declares an unscoped enum, so C++ callers use both
    # Tournament::ActionID::SetTextSize and Tournament::SetTextSize.
    EnableFlag = ActionID.EnableFlag
    PauseGame = ActionID.PauseGame
    ResumeGame = ActionID.ResumeGame
    LeaveGame = ActionID.LeaveGame
    SetLocalSpeed = ActionID.SetLocalSpeed
    SetTextSize = ActionID.SetTextSize
    SetLatCom = ActionID.SetLatCom
    SetGUI = ActionID.SetGUI
    SetMap = ActionID.SetMap
    SetFrameSkip = ActionID.SetFrameSkip
    Printf = ActionID.Printf
    SendText = ActionID.SendText
    SetCommandOptimizationLevel = ActionID.SetCommandOptimizationLevel


__all__ = ["BWAPI_getRevision", "BWAPI_isDebug", "Tournament"]
