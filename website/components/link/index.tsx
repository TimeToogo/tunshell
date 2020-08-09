import NextLink, { LinkProps } from "next/link";
import * as Styled from "./styled";

export const Link: React.FC<LinkProps> = ({ children, ...props }) => {
  return (
    <Styled.Link>
      <NextLink {...props}>
        <a>{children}</a>
      </NextLink>
    </Styled.Link>
  );
};
