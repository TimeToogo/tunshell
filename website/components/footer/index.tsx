import React from "react";
import { Container } from "../layout";
import * as Styled from "./styled";

export const Footer = () => {
  return (
    <Styled.Footer>
      <Container>
        <Styled.Contents>
          <Styled.Credits>
            <ion-icon name="heart"></ion-icon>
          </Styled.Credits>
        </Styled.Contents>
      </Container>
    </Styled.Footer>
  );
};
