import { useCallback, useEffect, useRef, useState } from "react";

import { ChevronLeftIcon } from "./icons";

type SlideToConfirmProps = {
  /** What the slider says while it waits to be dragged. */
  label: string;
  /** What it says once it has been carried to the end. */
  doneLabel: string;
  /** Shown instead of the label while the track is still closed. */
  waitingLabel?: string;
  disabled: boolean;
  onConfirm: () => void;
};

/** How far along the track counts as having arrived. */
const THRESHOLD = 0.92;

/** How much of the track an arrow key covers, for anybody not using a finger. */
const KEY_STEP = 0.2;

/**
 * A confirmation that cannot be tapped by accident: the knob has to be pressed
 * and carried the width of the track.
 *
 * A destructive act that a stray finger can complete is one that stray fingers
 * will complete. The keyboard is not left out — arrow keys walk the same
 * distance, which is also what a screen reader announces.
 */
export function SlideToConfirm({
  label,
  doneLabel,
  waitingLabel,
  disabled,
  onConfirm,
}: SlideToConfirmProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const startRef = useRef(0);
  const [progress, setProgress] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [done, setDone] = useState(false);

  // A track that closes again forgets how far anybody had carried it.
  useEffect(() => {
    if (disabled) {
      setProgress(0);
      setDone(false);
    }
  }, [disabled]);

  const travel = useCallback(() => {
    const track = trackRef.current;
    if (!track) {
      return 1;
    }
    // The knob is square and as tall as the track, so what is left to cross is
    // the track minus itself.
    return Math.max(1, track.clientWidth - track.clientHeight);
  }, []);

  const arrive = useCallback(() => {
    setProgress(1);
    setDone(true);
    onConfirm();
  }, [onConfirm]);

  function move(clientX: number) {
    const next = Math.min(1, Math.max(0, (clientX - startRef.current) / travel()));
    setProgress(next);
  }

  function release() {
    setDragging(false);
    if (progress >= THRESHOLD) {
      arrive();
    } else {
      setProgress(0);
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (disabled || done) {
      return;
    }
    if (event.key === "ArrowRight" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = Math.min(1, progress + KEY_STEP);
      if (next >= THRESHOLD) {
        arrive();
      } else {
        setProgress(next);
      }
    } else if (event.key === "ArrowLeft" || event.key === "ArrowDown") {
      event.preventDefault();
      setProgress(Math.max(0, progress - KEY_STEP));
    } else if (event.key === "End") {
      event.preventDefault();
      arrive();
    } else if (event.key === "Home") {
      event.preventDefault();
      setProgress(0);
    }
  }

  const caption = disabled && waitingLabel ? waitingLabel : done ? doneLabel : label;

  return (
    <div
      ref={trackRef}
      className={disabled ? "slide slide--closed" : "slide"}
      data-dragging={dragging ? "true" : undefined}
    >
      <span className="slide__fill" style={{ ["--progress" as string]: progress }} aria-hidden="true" />
      <span className="slide__caption">{caption}</span>
      <span
        className="slide__knob"
        style={{ ["--progress" as string]: progress }}
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-disabled={disabled}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(progress * 100)}
        onKeyDown={onKeyDown}
        onPointerDown={(event) => {
          if (disabled || done) {
            return;
          }
          event.currentTarget.setPointerCapture(event.pointerId);
          startRef.current = event.clientX - progress * travel();
          setDragging(true);
        }}
        onPointerMove={(event) => {
          if (dragging) {
            move(event.clientX);
          }
        }}
        onPointerUp={() => {
          if (dragging) {
            release();
          }
        }}
        onPointerCancel={() => {
          if (dragging) {
            setDragging(false);
            setProgress(0);
          }
        }}
      >
        {/* The chevron is the back arrow, turned to point the way out. */}
        <ChevronLeftIcon className="slide__arrow" />
      </span>
    </div>
  );
}
