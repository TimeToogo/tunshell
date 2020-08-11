import * as Styled from "./styled";
import { HTMLAttributes } from "react";

interface Props {
  mode?: "inverted" | "normal";
}

export const Button: React.FC<React.ButtonHTMLAttributes<HTMLButtonElement> & Props> = ({ children, ...props }) => {
  return <Styled.Button {...props}>{children}</Styled.Button>;
};
