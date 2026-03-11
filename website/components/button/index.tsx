import * as Styled from "./styled";
import { ButtonHTMLAttributes, PropsWithChildren } from "react";

interface Props {
  mode?: "inverted" | "normal";
}

type ButtonProps = PropsWithChildren<ButtonHTMLAttributes<HTMLButtonElement> & Props>;

export const Button = ({ children, ...props }: ButtonProps) => {
  return <Styled.Button {...props}>{children}</Styled.Button>;
};
