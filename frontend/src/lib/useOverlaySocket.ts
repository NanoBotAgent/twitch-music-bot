"use client";

import { useEffect, useRef, useState } from "react";
import { wsUrl } from "./api";
import type { OverlayMessage } from "./types";

/** Subscribes to the overlay WebSocket and keeps the latest message in state. */
export function useOverlaySocket(streamerId: string) {
  const [connected, setConnected] = useState(false);
  const [lastMessage, setLastMessage] = useState<OverlayMessage | null>(null);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!streamerId) return;

    let socket: WebSocket | null = null;
    let disposed = false;

    function connect() {
      if (disposed) return;
      socket = new WebSocket(wsUrl(`/api/v1/overlay/${streamerId}/ws`));

      socket.onopen = () => setConnected(true);
      socket.onclose = () => {
        setConnected(false);
        retryRef.current = setTimeout(connect, 3000);
      };
      socket.onerror = () => socket?.close();
      socket.onmessage = (event) => {
        try {
          setLastMessage(JSON.parse(event.data as string) as OverlayMessage);
        } catch {
          // ignore malformed frames
        }
      };
    }

    connect();

    return () => {
      disposed = true;
      if (retryRef.current) clearTimeout(retryRef.current);
      socket?.close();
    };
  }, [streamerId]);

  return { connected, lastMessage };
}
