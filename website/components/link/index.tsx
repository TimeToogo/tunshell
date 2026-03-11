import React, { AnchorHTMLAttributes, PropsWithChildren } from "react";
import NextLink, { LinkProps } from "next/link";
import * as Styled from "./styled";

type Props = PropsWithChildren<
  LinkProps & {
    a?: AnchorHTMLAttributes<HTMLAnchorElement>;
  }
>;

export const Link = ({ children, a = {}, ...props }: Props) => {
  return (
    <Styled.Link>
      <NextLink legacyBehavior passHref {...props}>
        <a {...a}>{children}</a>
      </NextLink>
    </Styled.Link>
  );
};
