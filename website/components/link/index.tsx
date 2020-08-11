import NextLink, { LinkProps } from "next/link";
import * as Styled from "./styled";

interface Props extends LinkProps {
  a?: React.AnchorHTMLAttributes<HTMLAnchorElement>;
}

export const Link: React.FC<Props> = ({ children, a = {}, ...props }) => {
  return (
    <Styled.Link>
      <NextLink {...props}>
        <a {...a}>{children}</a>
      </NextLink>
    </Styled.Link>
  );
};
