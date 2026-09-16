type ScanFramingGuideProps = {
  /** Shown inside the frame, telling the person what to do. */
  instruction: string;
  /** Second line, under the frame. */
  hint: string;
};

/**
 * Framing guide drawn over the camera preview.
 *
 * The camera picture is drawn by the operating system behind a transparent
 * webview, so everything here is deliberately see-through except the frame
 * itself: the dimmed surround is one large shadow cast outwards by the cutout.
 */
export function ScanFramingGuide({ instruction, hint }: ScanFramingGuideProps) {
  return (
    <div className="framing" role="presentation">
      <p className="framing__instruction">{instruction}</p>
      <div className="framing__frame">
        <span className="framing__corner framing__corner--tl" />
        <span className="framing__corner framing__corner--tr" />
        <span className="framing__corner framing__corner--br" />
        <span className="framing__corner framing__corner--bl" />
        <span className="framing__sweep" />
      </div>
      <p className="framing__hint">{hint}</p>
    </div>
  );
}
