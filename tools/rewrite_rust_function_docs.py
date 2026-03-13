from __future__ import annotations

import html
import os
import re
from pathlib import Path
from typing import List, Optional, Tuple

ROOT = Path(r"d:\naoto\programming\altpaint")
TARGET_DOC = ROOT / "target" / "doc"
INDEX_HTML = ROOT / "docs" / "rustdoc-index.html"
TARGET_INDEX_HTML = TARGET_DOC / "all-docs.html"

RS_FILES = [
    path
    for path in ROOT.rglob("*.rs")
    if "target" not in path.parts and ".git" not in path.parts
]

FN_RE = re.compile(
    r"^(?P<indent>[ \t]*)(?:pub(?:\s*\([^)]+\))?\s+)?(?:(?:const|async|unsafe|default)\s+)*(?:extern\s+\"[^\"]+\"\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\b"
)
ATTR_RE = re.compile(r"^[ \t]*#\[")
DOC_RE = re.compile(r"^[ \t]*///")
TEST_ATTR_RE = re.compile(r"^[ \t]*#\[(?:\w+::)?test\b")
HOST_CALL_RE = re.compile(r'host_(string|i32|bool|f32|json)\("([^"]+)"\)')
STATE_CALL_RE = re.compile(r'(set_state_(?:string|bool|i32|f32)|toggle_state)\(')
EMIT_SERVICE_RE = re.compile(r'emit_service\(&([^\)\n]+)\)')
EMIT_COMMAND_RE = re.compile(r'emit_command\(&([^\)\n]+)\)')
FIELD_RETURN_RE = re.compile(r'^\s*(?:self\.|Self::)?([A-Za-z_][A-Za-z0-9_\.]+)\s*$')
CALL_RE = re.compile(r'([A-Za-z_][A-Za-z0-9_:]*)\(')

WORD_MAP = {
    "active": "アクティブ",
    "add": "追加",
    "alpha": "アルファ",
    "antialias": "アンチエイリアス",
    "app": "アプリ",
    "apply": "適用",
    "background": "背景",
    "bitmap": "ビットマップ",
    "blend": "ブレンド",
    "bounds": "範囲",
    "brush": "ブラシ",
    "build": "構築",
    "canvas": "キャンバス",
    "capture": "取得",
    "catalog": "カタログ",
    "chunk": "チャンク",
    "clamp": "補正",
    "color": "色",
    "command": "コマンド",
    "compose": "合成",
    "config": "設定",
    "context": "コンテキスト",
    "count": "件数",
    "create": "生成",
    "creation": "生成",
    "current": "現在",
    "default": "既定",
    "descriptor": "記述子",
    "dialog": "ダイアログ",
    "dimension": "寸法",
    "dirty": "差分",
    "dispatch": "振り分け",
    "display": "表示",
    "document": "ドキュメント",
    "draw": "描画",
    "drawing": "描画",
    "edit": "編集",
    "engine": "エンジン",
    "eraser": "消しゴム",
    "error": "エラー",
    "event": "イベント",
    "export": "書き出し",
    "fill": "塗りつぶし",
    "filter": "絞り込み",
    "focus": "フォーカス",
    "font": "フォント",
    "format": "形式",
    "frame": "フレーム",
    "gesture": "ジェスチャ",
    "handler": "ハンドラ",
    "height": "高さ",
    "hex": "16進文字列",
    "host": "ホスト",
    "hsv": "HSV",
    "id": "ID",
    "import": "読み込み",
    "index": "インデックス",
    "init": "初期化",
    "input": "入力",
    "json": "JSON",
    "keyboard": "キーボード",
    "label": "ラベル",
    "lasso": "投げ縄",
    "layer": "レイヤー",
    "layout": "レイアウト",
    "list": "一覧",
    "load": "読込",
    "mask": "マスク",
    "memory": "メモリ",
    "mode": "モード",
    "name": "名前",
    "new": "新規",
    "next": "次",
    "offset": "オフセット",
    "opacity": "不透明度",
    "open": "開く",
    "option": "オプション",
    "options": "オプション",
    "overlay": "オーバーレイ",
    "page": "ページ",
    "palette": "パレット",
    "panel": "パネル",
    "parse": "解析",
    "path": "パス",
    "pen": "ペン",
    "pixel": "ピクセル",
    "plugin": "プラグイン",
    "png": "PNG",
    "point": "点",
    "preferred": "推奨",
    "present": "提示",
    "preview": "プレビュー",
    "previous": "前",
    "project": "プロジェクト",
    "rect": "矩形",
    "redraw": "再描画",
    "region": "領域",
    "reload": "再読込",
    "remove": "削除",
    "render": "描画",
    "replace": "置換",
    "request": "要求",
    "reset": "初期化",
    "resize": "リサイズ",
    "restore": "復元",
    "result": "結果",
    "rgb": "RGB",
    "rgba": "RGBA",
    "rotation": "回転",
    "route": "経路",
    "run": "実行",
    "save": "保存",
    "scale": "拡大率",
    "scene": "シーン",
    "scroll": "スクロール",
    "select": "選択",
    "selected": "選択中",
    "service": "サービス",
    "session": "セッション",
    "set": "設定",
    "settings": "設定",
    "shader": "シェーダ",
    "shortcut": "ショートカット",
    "show": "表示",
    "size": "サイズ",
    "snapshot": "スナップショット",
    "source": "ソース",
    "stamp": "スタンプ",
    "state": "状態",
    "status": "ステータス",
    "storage": "保存",
    "stroke": "ストローク",
    "surface": "サーフェス",
    "sync": "同期",
    "template": "テンプレート",
    "text": "テキスト",
    "tip": "先端形状",
    "toggle": "切替",
    "tool": "ツール",
    "transform": "変換",
    "update": "更新",
    "upload": "アップロード",
    "uv": "UV",
    "validate": "検証",
    "value": "値",
    "view": "ビュー",
    "visible": "表示状態",
    "wheel": "ホイール",
    "width": "幅",
    "window": "ウィンドウ",
    "workspace": "ワークスペース",
    "wrap": "折り返し",
    "write": "書き込み",
    "zoom": "ズーム",
}

GENERIC_PATTERNS = (
    "を実行します。",
    "を設定します。",
    "で値を構築します。",
    "で入力を解析します。",
    "で処理を振り分けます。",
    "で状態を更新します。",
    "でデータを書き出します。",
    "でデータを読み込みます。",
    "で状態を切り替えます。",
    "の変換結果を返します。",
    "の判定結果を返します。",
)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


class FunctionInfo:
    def __init__(self, path: Path, lines: List[str], index: int) -> None:
        self.path = path
        self.lines = lines
        self.index = index
        self.signature_line = lines[index]
        match = FN_RE.match(self.signature_line)
        assert match is not None
        self.indent = match.group("indent")
        self.name = match.group("name")
        self.attr_start = index
        self.doc_start: Optional[int] = None
        self.doc_end: Optional[int] = None
        self.is_test = False
        self.body_start = self._find_body_start()
        self.body_end = self._find_body_end()
        self.body = "\n".join(lines[self.body_start : self.body_end + 1]) if self.body_start is not None and self.body_end is not None else ""
        self._scan_leading_blocks()

    def _scan_leading_blocks(self) -> None:
        k = self.index - 1
        while k >= 0 and self.lines[k].strip() == "":
            k -= 1
        attr_start = self.index
        while k >= 0 and ATTR_RE.match(self.lines[k]):
            if TEST_ATTR_RE.match(self.lines[k]):
                self.is_test = True
            attr_start = k
            k -= 1
        self.attr_start = attr_start
        doc_end = k
        while k >= 0 and DOC_RE.match(self.lines[k]):
            k -= 1
        if doc_end >= 0 and doc_end != k:
            self.doc_start = k + 1
            self.doc_end = doc_end

    def _find_body_start(self) -> Optional[int]:
        brace_line = self.index
        while brace_line < len(self.lines):
            if "{" in self.lines[brace_line]:
                return brace_line
            brace_line += 1
        return None

    def _find_body_end(self) -> Optional[int]:
        if self.body_start is None:
            return None
        depth = 0
        in_string = False
        string_char = ""
        escape = False
        in_block_comment = 0
        for line_idx in range(self.body_start, len(self.lines)):
            line = self.lines[line_idx]
            i = 0
            while i < len(line):
                ch = line[i]
                nxt = line[i + 1] if i + 1 < len(line) else ""
                if in_string:
                    if escape:
                        escape = False
                    elif ch == "\\":
                        escape = True
                    elif ch == string_char:
                        in_string = False
                    i += 1
                    continue
                if in_block_comment:
                    if ch == "*" and nxt == "/":
                        in_block_comment -= 1
                        i += 2
                        continue
                    elif ch == "/" and nxt == "*":
                        in_block_comment += 1
                        i += 2
                        continue
                    i += 1
                    continue
                if ch == "/" and nxt == "/":
                    break
                if ch == "/" and nxt == "*":
                    in_block_comment += 1
                    i += 2
                    continue
                if ch in ('"', "'"):
                    in_string = True
                    string_char = ch
                    i += 1
                    continue
                if ch == "{":
                    depth += 1
                elif ch == "}":
                    depth -= 1
                    if depth == 0:
                        return line_idx
                i += 1
        return None

    def existing_doc(self) -> str:
        if self.doc_start is None or self.doc_end is None:
            return ""
        return "\n".join(self.lines[self.doc_start : self.doc_end + 1])


def collect_functions(path: Path) -> List[FunctionInfo]:
    lines = read_text(path).splitlines()
    result: List[FunctionInfo] = []
    for idx, line in enumerate(lines):
        if FN_RE.match(line):
            result.append(FunctionInfo(path, lines, idx))
    return result


def compact_body(body: str) -> str:
    body = re.sub(r"//.*", "", body)
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    body = re.sub(r"\s+", " ", body).strip()
    return body[:1200]


def snake_words(name: str) -> List[str]:
    return [part for part in name.split("_") if part]


def humanize_name(name: str) -> str:
    words = snake_words(name)
    if not words:
        return name
    translated = [WORD_MAP.get(word, word) for word in words]
    return " ".join(translated)


def normalize_symbol(symbol: str) -> str:
    symbol = re.sub(r"\s+", " ", symbol).strip().lstrip("&")
    symbol = symbol.split("(", 1)[0]
    symbol = symbol.replace("services::", "").replace("commands::", "")
    symbol = symbol.replace("::", " ")
    symbol = symbol.replace("_", " ")
    words = [WORD_MAP.get(word, word) for word in symbol.split()]
    return " ".join(words)


def signature_returns_value(signature: str) -> bool:
    return "->" in signature and not re.search(r"->\s*\(\s*\)", signature)


def test_summary(name: str) -> str:
    phrase = humanize_name(name)
    if phrase:
        return f"{phrase} が期待どおりに動作することを検証する。"
    return f"`{name}` の振る舞いを検証する。"


def detect_name_specific(info: FunctionInfo, body: str) -> Optional[str]:
    name = info.name
    attrs = []
    for idx in range(info.attr_start, info.index):
        attrs.append(info.lines[idx])
    attr_text = "\n".join(attrs)
    if name == "main":
        return "アプリケーションのエントリーポイントとしてランタイムを起動する。"
    if name == "run":
        return "イベントループを開始し、デスクトップ実行を継続する。"
    if name == "init":
        if "panel_init" in attr_text:
            return "パネル初期化時に必要な状態を整える。"
        return "初期化処理を行う。"
    if name == "sync_host":
        return "host snapshot を読み取り、表示用の状態へ同期する。"
    if name == "keyboard":
        return "キーボード入力やショートカットに応じて状態と処理を切り替える。"
    if name == "request_redraw":
        return "次のフレームで再描画が行われるよう要求する。"
    if name == "resize":
        return "描画先のサイズ変更を反映する。"
    if name == "default":
        return "既定値を持つインスタンスを返す。"
    if name == "from":
        return "別形式の値から現在の型へ変換する。"
    if name == "from_db":
        return "保存済み文字列を列挙値へ復元する。"
    if name in {"width", "height", "len"}:
        return f"現在の {humanize_name(name)} を返す。"
    if name == "contains" or name.startswith("contains_"):
        subject = humanize_name(name[9:] if name.startswith("contains_") else "対象")
        return f"{subject} が範囲内に含まれるか判定する。"
    if name.startswith("default_"):
        return f"既定の {humanize_name(name[8:])} を返す。"
    if name.startswith("active_") and signature_returns_value(info.signature_line):
        if name.endswith("_mut"):
            return f"アクティブな {humanize_name(name[7:-4])} への可変参照を返す。"
        return f"アクティブな {humanize_name(name[7:])} を返す。"
    if name.startswith("selected_") and signature_returns_value(info.signature_line):
        return f"選択中の {humanize_name(name[9:])} を返す。"
    if name.startswith("nearest_") and signature_returns_value(info.signature_line):
        return f"最も近い {humanize_name(name[8:])} を返す。"
    if name.startswith("resolved_"):
        return f"解決済みの {humanize_name(name[9:])} を返す。"
    if name.startswith("effective_"):
        return f"実効的な {humanize_name(name[10:])} を返す。"
    if name.startswith("preferred_"):
        return f"推奨される {humanize_name(name[10:])} を返す。"
    if name.startswith("fit_"):
        return f"{humanize_name(name[4:])} が収まるように矩形を計算する。"
    if name.startswith("map_"):
        return f"{humanize_name(name[4:])} を別座標系へ変換する。"
    if name.startswith("merge_"):
        return f"{humanize_name(name[6:])} を統合する。"
    if name.startswith("cycle_"):
        return f"{humanize_name(name[6:])} を順送りで切り替える。"
    if name.startswith("focus_"):
        return f"{humanize_name(name[6:])} へフォーカスを移す。"
    if name.startswith("record_"):
        return f"{humanize_name(name[7:])} を記録する。"
    if name.startswith("advance_"):
        return f"{humanize_name(name[8:])} を進行させる。"
    if name.startswith("rebuild_"):
        return f"{humanize_name(name[8:])} を再構築する。"
    if name.startswith("ensure_"):
        return f"{humanize_name(name[7:])} が満たされるよう整える。"
    if name.startswith("relayout_"):
        return f"{humanize_name(name[9:])} を再配置する。"
    if name.startswith("skip_"):
        return f"{humanize_name(name[5:])} を読み飛ばす。"
    if name.startswith("persist_"):
        return f"{humanize_name(name[8:])} を永続化する。"
    if name.startswith("take_"):
        return f"{humanize_name(name[5:])} を取り出して返す。"
    if name.startswith("flush_"):
        return f"保留中の {humanize_name(name[6:])} を反映する。"
    if name.startswith("defer_"):
        return f"{humanize_name(name[6:])} を後段の処理へ遅延させる。"
    if name.startswith("upload_"):
        return f"{humanize_name(name[7:])} を GPU へアップロードする。"
    if name.startswith("select_"):
        return f"{humanize_name(name[7:])} を選択状態へ更新する。"
    if name.startswith("add_"):
        return f"{humanize_name(name[4:])} を追加する。"
    if name.startswith("remove_"):
        return f"{humanize_name(name[7:])} を削除する。"
    if name.startswith("replace_"):
        return f"{humanize_name(name[8:])} を置き換える。"
    if name == "vs_main":
        return "頂点シェーダのエントリーポイントとして頂点出力を組み立てる。"
    if name == "fs_main":
        return "フラグメントシェーダのエントリーポイントとして最終色を返す。"
    if name.startswith("show_"):
        return f"{humanize_name(name[5:])} を表示できるよう状態を更新する。"
    if name.startswith("cancel_"):
        return f"{humanize_name(name[7:])} に関する表示や入力状態を閉じる。"
    if name.startswith("capture_") and "shortcut" in name:
        return f"{humanize_name(name[8:])} 用のショートカット入力を受け付ける状態にする。"
    if name == "select_template":
        return "選択されたテンプレートサイズを入力欄へ反映する。"
    if name == "new_project":
        return "入力済みサイズから新規プロジェクト作成要求を発行する。"
    if name == "save_project":
        return "現在のプロジェクトを既存パスへ保存する要求を発行する。"
    if name == "save_project_as":
        return "保存先を選んでプロジェクトを書き出す要求を発行する。"
    if name == "load_project":
        return "読み込み対象を選んでプロジェクトを開く要求を発行する。"
    if name.startswith("activate_"):
        return f"{humanize_name(name[9:])} をアクティブ化する。"
    if name.startswith("previous_"):
        return f"{humanize_name(name[9:])} をひとつ前へ切り替える。"
    if name.startswith("next_"):
        return f"{humanize_name(name[5:])} をひとつ先へ切り替える。"
    return None


def literal_host_summary(body: str) -> Optional[str]:
    m = HOST_CALL_RE.search(body)
    if not m:
        return None
    kind, key = m.groups()
    label = key.replace(".", " / ")
    suffix = {
        "string": "文字列",
        "i32": "整数値",
        "bool": "真偽値",
        "f32": "浮動小数点値",
        "json": "JSON 値",
    }.get(kind, "値")
    return f"host snapshot の {label} を {suffix}として返す。"


def detect_constructor(name: str, body: str, return_hint: str) -> Optional[str]:
    if name == "new" or re.search(r"\bSelf\s*\{", body):
        if "default" in body.lower():
            return "既定値を使って新しいインスタンスを生成する。"
        return "入力値を束ねた新しいインスタンスを生成する。"
    if name.startswith(("build_", "create_")):
        target = humanize_name(name.split("_", 1)[1] if "_" in name else name)
        if "Err(" in body or "?" in body:
            return f"{target} を構築し、失敗時はエラーを返す。"
        return f"{target} を構築する。"
    return None


def detect_parse(name: str, body: str) -> Optional[str]:
    if name.startswith(("parse_", "deserialize_")) or any(token in body for token in ["parse::<", "serde_json::from_", "split_once(", "from_str("]):
        subject = humanize_name(name.split("_", 1)[1] if "_" in name else "input")
        if "Err(" in body or "ok_or" in body or "map_err" in body:
            return f"入力を解析して {subject} に変換し、失敗時はエラーを返す。"
        return f"入力を解析して {subject} に変換する。"
    return None


def detect_serialize(name: str, body: str) -> Optional[str]:
    if name.startswith(("serialize_", "encode_")) or any(token in body for token in ["serde_json::to_", "to_string()", "to_vec()"]):
        target = humanize_name(name.split("_", 1)[1] if "_" in name else "output")
        return f"現在の値を {target} へ変換する。"
    if name.startswith(("to_", "into_", "as_", "hex_")):
        target = humanize_name(name.split("_", 1)[1] if "_" in name else "output")
        return f"現在の値を {target} 形式へ変換する。"
    return None


def detect_dispatch(name: str, body: str) -> Optional[str]:
    if name.startswith(("handle_", "dispatch_", "execute_")) or ("match " in body and body.count("=>") >= 2):
        if "emit_service" in body or "emit_command" in body:
            return "入力内容を判別し、必要な状態更新とサービス呼び出しへ振り分ける。"
        return "入力や種別に応じて処理を振り分ける。"
    return None


def detect_getter(name: str, body: str) -> Optional[str]:
    if name.startswith(("is_", "has_", "can_", "supports_", "should_")):
        subject = humanize_name(name)
        if "clamp" in body or ".min(" in body or ".max(" in body:
            return f"{subject} の判定結果を補正付きで返す。"
        return f"{subject} かどうかを返す。"
    if name.endswith(("_json", "_count", "_index", "_label", "_name", "_path", "_size", "_width", "_height", "_bounds", "_status")):
        return f"現在の {humanize_name(name)} を返す。"
    compact = compact_body(body)
    if len(compact) < 120:
        field_like = FIELD_RETURN_RE.match(compact.strip("{} ;"))
        if field_like:
            return f"現在の {humanize_name(name)} を返す。"
        if any(token in compact for token in [".clone()", ".copied()", ".as_ref()", "Some(", "None"]):
            return f"現在の {humanize_name(name)} を返す。"
    return None


def detect_setter(name: str, body: str) -> Optional[str]:
    if name.startswith("set_"):
        subject = humanize_name(name[4:])
        if "emit_service" in body or "emit_command" in body:
            return f"{subject} を更新し、関連するコマンドやサービス要求も発行する。"
        if STATE_CALL_RE.search(body):
            return f"状態上の {subject} を更新する。"
        return f"{subject} を設定する。"
    if name.startswith("toggle_"):
        subject = humanize_name(name[7:])
        if STATE_CALL_RE.search(body):
            return f"状態上の {subject} を切り替える。"
        return f"{subject} の有効状態を切り替える。"
    if name.startswith(("update_", "refresh_", "sync_", "mark_", "remember_", "restore_", "assign_", "apply_")):
        subject = humanize_name(name.split("_", 1)[1] if "_" in name else name)
        if "dirty" in body:
            return f"{subject} を更新し、必要な dirty 状態も記録する。"
        if "emit_service" in body or "emit_command" in body:
            return f"{subject} を反映し、必要なコマンドやサービス要求を発行する。"
        if name.startswith("sync_"):
            return f"{subject} を現在の状態へ同期する。"
        if name.startswith("apply_"):
            return f"{subject} を現在の状態へ適用する。"
        return f"{subject} を更新する。"
    return None


def detect_io(name: str, body: str) -> Optional[str]:
    io_markers = ["File::open", "File::create", "fs::", "Connection::open", "read_to_end", "write_all", "transaction", "save_", "load_"]
    if name.startswith(("save_", "load_", "read_", "write_", "open_", "export_", "import_")) or any(marker in body for marker in io_markers):
        subject = humanize_name(name.split("_", 1)[1] if "_" in name else "data")
        if name.startswith(("save_", "write_", "export_")):
            return f"{subject} を保存先へ書き出す。"
        if name.startswith(("load_", "read_", "open_", "import_")):
            return f"{subject} を読み込み、必要に応じて整形して返す。"
    return None


def detect_render(name: str, body: str) -> Optional[str]:
    render_tokens = ["RenderFrame", "fill_rect", "blit_", "draw_", "overlay", "dirty_rect", "dirty rect", "CanvasDirtyRect"]
    if name.startswith(("render_", "compose_", "draw_", "fill_", "blit_", "stroke_", "scroll_")) or any(token in body for token in render_tokens):
        target = humanize_name(name)
        if "dirty" in body.lower():
            return f"{target} に必要な差分領域だけを描画または合成する。"
        return f"{target} に必要な描画内容を組み立てる。"
    return None


def detect_bitmap(name: str, body: str) -> Optional[str]:
    if any(token in body for token in ["BitmapEdit", "CanvasBitmap", "CanvasDirtyRect", "rgba", "pixel", "stamp", "lasso", "flood_fill"]):
        if "for " in body and body.count("for ") >= 2:
            return f"ピクセル走査を行い、{humanize_name(name)} 用のビットマップ結果を生成する。"
        return f"{humanize_name(name)} に対応するビットマップ処理を行う。"
    return None


def detect_service(name: str, body: str) -> Optional[str]:
    service_match = EMIT_SERVICE_RE.search(body)
    if service_match:
        target = normalize_symbol(service_match.group(1))
        return f"{target} に対応するサービス要求を発行する。"
    command_match = EMIT_COMMAND_RE.search(body)
    if command_match:
        target = normalize_symbol(command_match.group(1))
        return f"{target} に対応するコマンドを発行する。"
    return None


def detect_len_misc(name: str, body: str) -> Optional[str]:
    compact = compact_body(body)
    if "clamp(" in compact:
        return f"{humanize_name(name)} を有効範囲へ補正して返す。"
    if "format!(" in compact:
        return f"{humanize_name(name)} 用の表示文字列を組み立てる。"
    if "split_once(" in compact or ".split(" in compact:
        return f"入力を分解して {humanize_name(name)} に必要な値を取り出す。"
    if any(token in compact for token in [".iter()", ".map(", ".collect("]):
        return f"既存データを走査して {humanize_name(name)} を組み立てる。"
    return None


def build_summary(info: FunctionInfo) -> str:
    body = info.body
    if info.is_test:
        return test_summary(info.name)
    for detector in (
        lambda b: detect_name_specific(info, b),
        literal_host_summary,
        lambda b: detect_constructor(info.name, b, info.signature_line),
        lambda b: detect_parse(info.name, b),
        lambda b: detect_serialize(info.name, b),
        lambda b: detect_dispatch(info.name, b),
        lambda b: detect_service(info.name, b),
        lambda b: detect_setter(info.name, b),
        lambda b: detect_io(info.name, b),
        lambda b: detect_render(info.name, b),
        lambda b: detect_bitmap(info.name, b),
        lambda b: detect_getter(info.name, b),
        lambda b: detect_len_misc(info.name, b),
    ):
        summary = detector(body)
        if summary:
            summary = summary[0].upper() + summary[1:] if summary and summary[0].islower() else summary
            return summary
    humanized = humanize_name(info.name)
    if signature_returns_value(info.signature_line):
        if info.name.endswith("_at"):
            return f"指定位置の {humanized[:-3].strip()} を計算して返す。"
        return f"{humanized} を計算して返す。"
    return f"{humanized} に必要な処理を行う。"


def build_extra(info: FunctionInfo) -> Optional[str]:
    body = compact_body(info.body)
    extras: List[str] = []
    if "Result<" in info.signature_line:
        if "Err(" in body or "?" in body:
            extras.append("失敗時はエラーを返します。")
    if "Option<" in info.signature_line:
        extras.append("値を生成できない場合は `None` を返します。")
    if "dirty" in body.lower() and all("dirty" not in e for e in extras):
        extras.append("必要に応じて dirty 状態も更新します。")
    if "emit_service" in body and all("サービス要求" not in e for e in extras):
        extras.append("内部でサービス要求を発行します。")
    if "emit_command" in body and all("コマンド" not in e for e in extras):
        extras.append("内部でコマンドを発行します。")
    return extras[0] if extras else None


def build_doc_block(info: FunctionInfo) -> List[str]:
    indent = info.indent
    summary = build_summary(info)
    extra = build_extra(info)
    lines = [f"{indent}/// {summary}"]
    if extra and extra not in summary:
        lines.append(f"{indent}///")
        lines.append(f"{indent}/// {extra}")
    return lines


def rewrite_file(path: Path) -> Tuple[bool, int]:
    lines = read_text(path).splitlines()
    functions = collect_functions(path)
    if not functions:
        return False, 0
    offset = 0
    changed = False
    replaced = 0
    for info in functions:
        current_index = info.index + offset
        doc_start = info.doc_start + offset if info.doc_start is not None else None
        doc_end = info.doc_end + offset if info.doc_end is not None else None
        attr_start = info.attr_start + offset
        new_block = build_doc_block(info)
        if doc_start is not None and doc_end is not None:
            old_block = lines[doc_start : doc_end + 1]
            if old_block == new_block:
                continue
            lines[doc_start : doc_end + 1] = new_block
            offset += len(new_block) - len(old_block)
            changed = True
            replaced += 1
        else:
            lines[attr_start:attr_start] = new_block
            offset += len(new_block)
            changed = True
            replaced += 1
    if changed:
        write_text(path, "\n".join(lines) + "\n")
    return changed, replaced


def is_generic_doc_line(line: str) -> bool:
    stripped = line.strip()
    return stripped.startswith("///") and any(pattern in stripped for pattern in GENERIC_PATTERNS)


def generate_index() -> None:
    TARGET_DOC.mkdir(parents=True, exist_ok=True)
    crate_dirs = sorted(
        p.name
        for p in TARGET_DOC.iterdir()
        if p.is_dir() and (p / "index.html").exists()
    )
    utility_links = [
        ("help.html", "rustdoc ヘルプ"),
        ("settings.html", "rustdoc 設定"),
    ]
    rows = []
    for crate in crate_dirs:
        label = crate.replace("_", "-")
        rows.append(f'<li><a href="{crate}/index.html">{html.escape(label)}</a></li>')
    utility_rows = []
    for href, label in utility_links:
        if (TARGET_DOC / href).exists():
            utility_rows.append(f'<li><a href="{href}">{html.escape(label)}</a></li>')
    page = f"""<!DOCTYPE html>
<html lang=\"ja\">
<head>
  <meta charset=\"utf-8\">
  <title>altpaint rustdoc index</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 2rem auto; max-width: 960px; line-height: 1.6; padding: 0 1rem; }}
    h1, h2 {{ margin-bottom: .4rem; }}
    .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 1.5rem; }}
    section {{ border: 1px solid #ddd; border-radius: 12px; padding: 1rem 1.2rem; background: #fafafa; }}
    ul {{ margin: .5rem 0 0; padding-left: 1.2rem; }}
    a {{ color: #0b57d0; text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .meta {{ color: #555; font-size: .95rem; }}
  </style>
</head>
<body>
  <h1>altpaint rustdoc 一覧</h1>
  <p class=\"meta\">workspace 全体の rustdoc へアクセスするための索引ページです。</p>
  <div class=\"grid\">
    <section>
      <h2>crate / plugin</h2>
      <ul>
        {''.join(rows)}
      </ul>
    </section>
    <section>
      <h2>補助ページ</h2>
      <ul>
        {''.join(utility_rows)}
      </ul>
    </section>
  </div>
</body>
</html>
"""
    write_text(INDEX_HTML, page)
    write_text(TARGET_INDEX_HTML, page)


def main() -> None:
    total_files = 0
    total_replaced = 0
    generic_before = 0
    for path in RS_FILES:
        for line in read_text(path).splitlines():
            if is_generic_doc_line(line):
                generic_before += 1
    for path in RS_FILES:
        changed, replaced = rewrite_file(path)
        if changed:
            total_files += 1
            total_replaced += replaced
    generate_index()
    generic_after = 0
    for path in RS_FILES:
        for line in read_text(path).splitlines():
            if is_generic_doc_line(line):
                generic_after += 1
    print(f"files_changed={total_files}")
    print(f"functions_rewritten={total_replaced}")
    print(f"generic_before={generic_before}")
    print(f"generic_after={generic_after}")
    print(f"index_html={INDEX_HTML}")
    print(f"target_index_html={TARGET_INDEX_HTML}")


if __name__ == "__main__":
    main()
