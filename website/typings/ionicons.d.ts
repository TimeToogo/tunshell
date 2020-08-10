import { HTMLAttributes } from "react";

declare global {
  namespace JSX {
    interface IntrinsicElements {
      "ion-icon": HTMLAttributes<HTMLDivElement> & { name: string };
    }
  }
}
