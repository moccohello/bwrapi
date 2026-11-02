from .types import GameType, GameTypes
from .type_ids import GameTypeId

# GameTypes also exposes the upper-case GameTypeId names.
for _name, _value in GameTypeId.__members__.items():
    setattr(GameTypes, _name, _value)

__all__ = ["GameType", "GameTypes"]
