"""Dump a rekordbox 6/7 library (master.db) to JSON for Stickwise.

Usage: python rekordbox_dump.py MASTER_DIR OUT.json

MASTER_DIR is rekordbox's library folder (the one holding master.db and
share/). The database is copied to a temporary folder first and opened
read-only through pyrekordbox; rekordbox's own files are never written.
"""

import json
import os
import shutil
import sys
import tempfile
import warnings

warnings.filterwarnings("ignore")

from pyrekordbox.masterdb import MasterDatabase  # noqa: E402
from pyrekordbox.masterdb.models import (  # noqa: E402
    DjmdContent,
    DjmdCue,
    DjmdPlaylist,
    DjmdSongPlaylist,
)

# rekordbox stores hot cues A–H as Kind 1, 2, 3, 5, 6, 7, 8, 9; 0 is a memory cue.
HOT_SLOT = {1: 1, 2: 2, 3: 3, 5: 4, 6: 5, 7: 6, 8: 7, 9: 8}


def name(obj, attr="Name"):
    return getattr(obj, attr, None) if obj is not None else None


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    master_dir, out_path = sys.argv[1], sys.argv[2]
    share = os.path.join(master_dir, "share")
    with tempfile.TemporaryDirectory() as tmp:
        for f in ("master.db", "master.db-wal", "master.db-shm"):
            src = os.path.join(master_dir, f)
            if os.path.exists(src):
                shutil.copy2(src, os.path.join(tmp, f))
        db = MasterDatabase(os.path.join(tmp, "master.db"))

        tracks = []
        for c in db.query(DjmdContent).all():
            if getattr(c, "rb_local_deleted", 0):
                continue
            anlz = c.AnalysisDataPath or ""
            art = c.ImagePath or ""
            tracks.append(
                {
                    "rb_id": str(c.ID),
                    "path": c.FolderPath,
                    "title": c.Title or "",
                    "artist": name(c.Artist) or "",
                    "album": name(c.Album) or "",
                    "genre": name(c.Genre) or "",
                    "label": name(c.Label) or "",
                    "remixer": name(c.Remixer) or "",
                    "key": name(c.Key, "ScaleName") or "",
                    "bpm": (c.BPM or 0) / 100.0,
                    "duration": c.Length or 0,
                    "bitrate": c.BitRate or 0,
                    "sample_rate": c.SampleRate or 0,
                    "sample_depth": c.BitDepth or 0,
                    "rating": c.Rating or 0,
                    "color": int(c.ColorID) if c.ColorID not in (None, "") else 0,
                    "comment": c.Commnt or "",
                    "year": c.ReleaseYear or 0,
                    "track_number": c.TrackNo or 0,
                    "date_added": str(c.DateCreated or "")[:10],
                    "anlz": os.path.join(share, anlz.lstrip("/")) if anlz else "",
                    "artwork": os.path.join(share, art.lstrip("/")) if art else "",
                }
            )

        cues = []
        for q in db.query(DjmdCue).all():
            if getattr(q, "rb_local_deleted", 0):
                continue
            if q.Kind != 0 and q.Kind not in HOT_SLOT:
                continue
            loop_end = q.OutMsec if q.OutMsec not in (None, -1) and q.OutMsec > q.InMsec else None
            bls = q.BeatLoopSize or 0
            cues.append(
                {
                    "rb_id": str(q.ContentID),
                    "slot": HOT_SLOT.get(q.Kind, 0),
                    "time_ms": max(0, q.InMsec or 0),
                    "loop_end_ms": loop_end,
                    "loop_beats": [bls >> 16, bls & 0xFFFF] if loop_end and bls else None,
                    "comment": q.Comment or "",
                    "hot_color": q.ColorTableIndex if q.Kind != 0 and (q.ColorTableIndex or 0) > 0 else 0,
                    "memory_color": q.Color if q.Kind == 0 and (q.Color or -1) > 0 else 0,
                    "active_loop": bool(q.ActiveLoop),
                }
            )

        playlists = []
        for p in db.query(DjmdPlaylist).all():
            if getattr(p, "rb_local_deleted", 0):
                continue
            # Attribute: 0 playlist, 1 folder, 4 smart playlist (skipped: rules, not lists).
            if p.Attribute not in (0, 1):
                continue
            playlists.append(
                {
                    "rb_id": str(p.ID),
                    "parent_rb_id": str(p.ParentID) if p.ParentID not in (None, "root") else "root",
                    "name": p.Name or "",
                    "is_folder": p.Attribute == 1,
                    "sort_order": p.Seq or 0,
                }
            )

        items = [
            {"playlist_rb_id": str(s.PlaylistID), "rb_id": str(s.ContentID), "position": s.TrackNo or 0}
            for s in db.query(DjmdSongPlaylist).all()
            if not getattr(s, "rb_local_deleted", 0)
        ]
        db.close()

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump({"tracks": tracks, "cues": cues, "playlists": playlists, "items": items}, f, ensure_ascii=False)
    print(json.dumps({"tracks": len(tracks), "cues": len(cues), "playlists": len(playlists), "items": len(items)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
